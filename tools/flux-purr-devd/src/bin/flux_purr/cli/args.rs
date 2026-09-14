#[derive(Debug, Parser)]
#[command(name = "flux-purr", version = flux_purr_devd::PRODUCT_VERSION)]
#[command(about = "Flux Purr CLI for USB/devd hardware workflows")]
struct Cli {
    #[arg(long, global = true, default_value = DEFAULT_DEVD_ENDPOINT)]
    devd: String,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Devices,
    Lan {
        #[command(subcommand)]
        command: LanCommand,
    },
    Identity(TargetSelector),
    Status(TargetSelector),
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },
    Buzzer {
        #[command(subcommand)]
        command: BuzzerCommand,
    },
    Pd {
        #[command(subcommand)]
        command: PdCommand,
    },
    Wifi {
        #[command(subcommand)]
        command: WifiCommand,
    },
    Calibration {
        #[command(subcommand)]
        command: CalibrationCommand,
    },
    CalibrationMode {
        #[command(subcommand)]
        command: CalibrationModeCommand,
    },
    HeaterCurve {
        #[command(subcommand)]
        command: HeaterCurveCommand,
    },
    Thermal {
        #[command(subcommand)]
        command: ThermalCommand,
    },
    Update(UpdateArgs),
    Flash(FlashArgs),
    Recover(RecoverArgs),
    Eeprom {
        #[command(subcommand)]
        command: EepromCommand,
    },
    Monitor(MonitorArgs),
    Hardware {
        #[command(subcommand)]
        command: HardwareCommand,
    },
    UsbPort {
        #[command(subcommand)]
        command: UsbPortCommand,
    },
}

#[derive(Debug, Subcommand)]
enum LanCommand {
    Devices,
    #[command(
        about = "Explicitly browse Flux Purr mDNS records. This never starts a background scan."
    )]
    Refresh,
    #[command(about = "Explicitly probe a private IPv4 CIDR (at most 256 hosts).")]
    Scan(LanScanArgs),
    Pair(LanPairArgs),
    #[command(
        about = "Read the current four-digit code through a USB/devd lease. The WiFi Info page must remain open."
    )]
    PairingCode(TargetSelector),
    #[command(
        about = "Open the physical WiFi Info pairing window through a USB/devd lease and return its four-digit code."
    )]
    PairingOpen(TargetSelector),
    #[command(
        about = "Close the physical WiFi Info pairing window through a USB/devd lease and invalidate its code."
    )]
    PairingClose(TargetSelector),
    #[command(
        about = "Clear a LAN token only through a USB/devd lease; the device must be selected explicitly."
    )]
    Reset(TargetSelector),
    Status(LanTargetArgs),
    RuntimeSet(LanRuntimeSetArgs),
    #[command(
        about = "Send a complete authorized LAN API operation. Writes acquire and release a temporary device lease."
    )]
    Request(LanRequestArgs),
}

#[derive(Debug, Args)]
struct LanPairArgs {
    #[arg(long = "url")]
    base_url: String,
    #[arg(
        long,
        help = "Required only when the connected device reports required pairing"
    )]
    code: Option<String>,
}

#[derive(Debug, Args)]
struct LanScanArgs {
    #[arg(long)]
    cidr: String,
}

#[derive(Debug, Args)]
struct LanTargetArgs {
    #[arg(long)]
    id: String,
}

#[derive(Debug, Args)]
struct LanRuntimeSetArgs {
    #[command(flatten)]
    target: LanTargetArgs,
    #[arg(long = "target-temp-c")]
    target_temp_c: Option<i16>,
    #[arg(long = "active-cooling")]
    active_cooling: Option<bool>,
    #[arg(long = "post-heat-cooling", value_parser = ["off", "normal", "fast"])]
    post_heat_cooling: Option<String>,
    #[arg(long = "heating-fan-guard", value_parser = ["off", "low", "medium", "high"])]
    heating_fan_guard: Option<String>,
    #[arg(long = "heater-enabled")]
    heater_enabled: Option<bool>,
}

#[derive(Debug, Args)]
struct LanRequestArgs {
    #[command(flatten)]
    target: LanTargetArgs,
    #[arg(long, value_enum)]
    method: LanHttpMethod,
    #[arg(
        long,
        help = "API path below /api/v1, for example calibration or thermal-profile."
    )]
    path: String,
    #[arg(
        long,
        conflicts_with = "body_file",
        help = "JSON request body for POST or PUT."
    )]
    body: Option<String>,
    #[arg(
        long = "body-file",
        conflicts_with = "body",
        help = "Path to a JSON request body."
    )]
    body_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LanHttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl LanHttpMethod {
    const fn as_reqwest(self) -> Method {
        match self {
            Self::Get => Method::GET,
            Self::Post => Method::POST,
            Self::Put => Method::PUT,
            Self::Delete => Method::DELETE,
        }
    }
}

#[derive(Debug, Args, Clone)]
struct TargetSelector {
    #[arg(long)]
    device: Option<String>,
    #[arg(long)]
    hardware: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum BenchSourceKind {
    Isolapurr,
}

impl BenchSourceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Isolapurr => "isolapurr",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum ThermalProfileMode {
    Auto,
    #[value(name = "65w")]
    W65,
    #[value(name = "100w")]
    W100,
}

impl ThermalProfileMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::W65 => "65w",
            Self::W100 => "100w",
        }
    }

    fn explicit_bank(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::W65 => Some("pps3a"),
            Self::W100 => Some("pps5a"),
        }
    }

    fn explicit_source_defaults(self) -> Option<(u16, u16)> {
        match self {
            Self::Auto => None,
            Self::W65 => Some((20_000, 3_250)),
            Self::W100 => Some((21_000, 5_000)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum ThermalSelfTestEvaluationMode {
    TuningScout,
    HoldConfirm,
}

impl ThermalSelfTestEvaluationMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::TuningScout => "tuning-scout",
            Self::HoldConfirm => "hold-confirm",
        }
    }

    fn enforces_stage_limits(self) -> bool {
        matches!(self, Self::HoldConfirm)
    }

    fn reports_stage_limits(self) -> bool {
        true
    }
}

#[derive(Debug, Subcommand)]
enum RuntimeCommand {
    Get(TargetSelector),
    Set(RuntimeSetArgs),
}

#[derive(Debug, Subcommand)]
enum BuzzerCommand {
    #[command(about = "Run a feature-gated, module-level buzzer test through a USB/devd lease.")]
    Test(BuzzerTestArgs),
    #[command(about = "Interactively select and play a feature-gated buzzer cue through USB/devd.")]
    Play(BuzzerPlayArgs),
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("buzzer_action")
        .required(true)
        .multiple(false)
        .args(["cue", "scenario", "stop", "status"])
))]
struct BuzzerTestArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long, value_enum)]
    cue: Option<BuzzerCueArg>,
    #[arg(long, value_enum)]
    scenario: Option<BuzzerScenarioArg>,
    #[arg(long, visible_alias = "loop", requires = "cue")]
    repeat: bool,
    #[arg(long)]
    stop: bool,
    #[arg(long)]
    status: bool,
}

#[derive(Debug, Args)]
struct BuzzerPlayArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(
        long,
        help = "Enable pointer capture at startup; turn it off with M to copy terminal text."
    )]
    pointer: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BuzzerCueArg {
    UiInput,
    HeaterOn,
    HeaterOff,
    ActiveCoolingOn,
    ActiveCoolingOff,
    HeaterReject,
    ActiveCoolingReject,
    ProtectionAlarm,
    AttentionReminder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuzzerInteractiveAction {
    Exit,
    Refresh,
    Stop,
    Play {
        cue: BuzzerCueArg,
        repeat: bool,
        stop_current: bool,
    },
    RunScenario {
        scenario: BuzzerScenarioArg,
        stop_current: bool,
    },
}

impl BuzzerCueArg {
    const fn wire_value(self) -> &'static str {
        match self {
            Self::UiInput => "ui_input",
            Self::HeaterOn => "heater_on",
            Self::HeaterOff => "heater_off",
            Self::ActiveCoolingOn => "active_cooling_on",
            Self::ActiveCoolingOff => "active_cooling_off",
            Self::HeaterReject => "heater_reject",
            Self::ActiveCoolingReject => "active_cooling_reject",
            Self::ProtectionAlarm => "protection_alarm",
            Self::AttentionReminder => "attention_reminder",
        }
    }

    const fn one_shot_duration_ms(self) -> u64 {
        match self {
            Self::UiInput => 45,
            Self::HeaterOn | Self::HeaterOff => 170,
            Self::ActiveCoolingOn | Self::ActiveCoolingOff => 210,
            Self::HeaterReject => 305,
            Self::ActiveCoolingReject => 310,
            Self::ProtectionAlarm => 300,
            // The cue pattern itself is 210 ms; the firmware owns the initial
            // start and ten-second replay cadence.
            Self::AttentionReminder => 210,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BuzzerCueDescriptor {
    cue: BuzzerCueArg,
    label: &'static str,
    kind: &'static str,
    rhythm: &'static str,
}

const BUZZER_CUE_CATALOG: [BuzzerCueDescriptor; 9] = [
    BuzzerCueDescriptor {
        cue: BuzzerCueArg::UiInput,
        label: "UI input",
        kind: "feedback",
        rhythm: "1080 Hz for 45 ms",
    },
    BuzzerCueDescriptor {
        cue: BuzzerCueArg::HeaterOn,
        label: "Heater on",
        kind: "feedback",
        rhythm: "1240 Hz 60 ms, 30 ms rest, 1680 Hz 80 ms",
    },
    BuzzerCueDescriptor {
        cue: BuzzerCueArg::HeaterOff,
        label: "Heater off",
        kind: "feedback",
        rhythm: "1680 Hz 60 ms, 30 ms rest, 1240 Hz 80 ms",
    },
    BuzzerCueDescriptor {
        cue: BuzzerCueArg::ActiveCoolingOn,
        label: "Active cooling on",
        kind: "feedback",
        rhythm: "900 / 1200 / 1550 Hz ascending",
    },
    BuzzerCueDescriptor {
        cue: BuzzerCueArg::ActiveCoolingOff,
        label: "Active cooling off",
        kind: "feedback",
        rhythm: "1550 / 1200 / 900 Hz descending",
    },
    BuzzerCueDescriptor {
        cue: BuzzerCueArg::HeaterReject,
        label: "Heater reject",
        kind: "feedback",
        rhythm: "420 Hz 120 ms, 35 ms rest, 360 Hz 150 ms",
    },
    BuzzerCueDescriptor {
        cue: BuzzerCueArg::ActiveCoolingReject,
        label: "Active cooling reject",
        kind: "feedback",
        rhythm: "480 Hz twice, then 320 Hz",
    },
    BuzzerCueDescriptor {
        cue: BuzzerCueArg::ProtectionAlarm,
        label: "Protection alarm",
        kind: "safety",
        rhythm: "2300 Hz 90 ms, 40 ms rest, 2300 Hz 90 ms; repeat cadence 1 s",
    },
    BuzzerCueDescriptor {
        cue: BuzzerCueArg::AttentionReminder,
        label: "Attention reminder",
        kind: "safety",
        rhythm: "1650 Hz 70 ms, 30 ms rest, 2200 Hz 110 ms; reminder cadence 10 s",
    },
];

#[derive(Debug, Clone, Copy)]
struct BuzzerScenarioDescriptor {
    scenario: BuzzerScenarioArg,
    label: &'static str,
    description: &'static str,
}

const BUZZER_SCENARIO_CATALOG: [BuzzerScenarioDescriptor; 3] = [
    BuzzerScenarioDescriptor {
        scenario: BuzzerScenarioArg::FeedbackCoalesce,
        label: "Feedback coalesce",
        description: "three UI-input requests at 0, 15, and 30 ms",
    },
    BuzzerScenarioDescriptor {
        scenario: BuzzerScenarioArg::FeedbackReplace,
        label: "Feedback replace",
        description: "two UI-input requests followed by heater-on at 30 ms",
    },
    BuzzerScenarioDescriptor {
        scenario: BuzzerScenarioArg::ActiveCoolingRetrigger,
        label: "Active cooling retrigger",
        description: "three active-cooling-on requests at 0, 15, and 30 ms",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BuzzerScenarioArg {
    FeedbackCoalesce,
    FeedbackReplace,
    ActiveCoolingRetrigger,
}

impl BuzzerScenarioArg {
    const fn wire_value(self) -> &'static str {
        match self {
            Self::FeedbackCoalesce => "feedback_coalesce",
            Self::FeedbackReplace => "feedback_replace",
            Self::ActiveCoolingRetrigger => "active_cooling_retrigger",
        }
    }

    const fn duration_ms(self) -> u64 {
        match self {
            Self::FeedbackCoalesce => 250,
            Self::FeedbackReplace => 350,
            Self::ActiveCoolingRetrigger => 500,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuzzerTerminalItem {
    Cue(BuzzerCueArg),
    Scenario(BuzzerScenarioArg),
}

const fn buzzer_terminal_item_count() -> usize {
    BUZZER_CUE_CATALOG.len() + BUZZER_SCENARIO_CATALOG.len()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct BuzzerTerminalSelection {
    index: usize,
}

impl BuzzerTerminalSelection {
    fn item(self) -> BuzzerTerminalItem {
        if let Some(descriptor) = BUZZER_CUE_CATALOG.get(self.index) {
            return BuzzerTerminalItem::Cue(descriptor.cue);
        }
        BuzzerTerminalItem::Scenario(
            BUZZER_SCENARIO_CATALOG[self.index - BUZZER_CUE_CATALOG.len()].scenario,
        )
    }

    fn move_previous(&mut self) {
        self.index = self.index.saturating_sub(1);
    }

    fn move_next(&mut self) {
        self.index = (self.index + 1).min(buzzer_terminal_item_count() - 1);
    }

    fn select_row(&mut self, row: u16) -> bool {
        let Some(index) = buzzer_terminal_item_index_at_row(row) else {
            return false;
        };
        self.index = index;
        true
    }

    fn primary_action(self, session_running: bool) -> BuzzerInteractiveAction {
        if session_running {
            return BuzzerInteractiveAction::Stop;
        }
        match self.item() {
            BuzzerTerminalItem::Cue(cue) => BuzzerInteractiveAction::Play {
                cue,
                repeat: false,
                stop_current: false,
            },
            BuzzerTerminalItem::Scenario(scenario) => BuzzerInteractiveAction::RunScenario {
                scenario,
                stop_current: false,
            },
        }
    }

    fn continuous_action(self, session_running: bool) -> Option<BuzzerInteractiveAction> {
        if session_running {
            return None;
        }
        match self.item() {
            BuzzerTerminalItem::Cue(cue) => Some(BuzzerInteractiveAction::Play {
                cue,
                repeat: true,
                stop_current: false,
            }),
            BuzzerTerminalItem::Scenario(_) => None,
        }
    }
}

const BUZZER_TERMINAL_CUE_START_ROW: u16 = 6;

const fn buzzer_terminal_scenario_start_row() -> u16 {
    BUZZER_TERMINAL_CUE_START_ROW + BUZZER_CUE_CATALOG.len() as u16 + 1
}

const fn buzzer_terminal_actions_row() -> u16 {
    buzzer_terminal_scenario_start_row() + BUZZER_SCENARIO_CATALOG.len() as u16 + 2
}

fn buzzer_terminal_item_index_at_row(row: u16) -> Option<usize> {
    let cue_end = BUZZER_TERMINAL_CUE_START_ROW + BUZZER_CUE_CATALOG.len() as u16;
    if (BUZZER_TERMINAL_CUE_START_ROW..cue_end).contains(&row) {
        return Some((row - BUZZER_TERMINAL_CUE_START_ROW) as usize);
    }

    let scenario_start = buzzer_terminal_scenario_start_row();
    let scenario_end = scenario_start + BUZZER_SCENARIO_CATALOG.len() as u16;
    if (scenario_start..scenario_end).contains(&row) {
        return Some(BUZZER_CUE_CATALOG.len() + (row - scenario_start) as usize);
    }
    None
}

fn buzzer_terminal_move_selection(
    selection: &mut BuzzerTerminalSelection,
    key: KeyCode,
    kind: KeyEventKind,
) -> bool {
    if !matches!(kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return false;
    }
    match key {
        KeyCode::Up => selection.move_previous(),
        KeyCode::Down => selection.move_next(),
        KeyCode::Home => selection.index = 0,
        KeyCode::End => selection.index = buzzer_terminal_item_count() - 1,
        _ => return false,
    }
    true
}

fn buzzer_terminal_key_action(
    key: KeyCode,
    kind: KeyEventKind,
    selection: BuzzerTerminalSelection,
    session_running: bool,
) -> Option<BuzzerInteractiveAction> {
    if !matches!(kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    match key {
        KeyCode::Enter | KeyCode::Char(' ') => Some(selection.primary_action(session_running)),
        KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Char('l') | KeyCode::Char('L') => {
            selection.continuous_action(session_running)
        }
        KeyCode::Char('s') | KeyCode::Char('S') => Some(BuzzerInteractiveAction::Stop),
        KeyCode::Char('r') | KeyCode::Char('R') => Some(BuzzerInteractiveAction::Refresh),
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
            Some(BuzzerInteractiveAction::Exit)
        }
        _ => None,
    }
}

fn buzzer_terminal_pointer_action(
    row: u16,
    column: u16,
    selection: BuzzerTerminalSelection,
    session_running: bool,
) -> Option<BuzzerInteractiveAction> {
    if row != buzzer_terminal_actions_row() {
        return None;
    }
    match column {
        0..=23 => Some(selection.primary_action(session_running)),
        24..=41 => selection.continuous_action(session_running),
        42..=51 => Some(BuzzerInteractiveAction::Stop),
        52..=65 => Some(BuzzerInteractiveAction::Refresh),
        66.. => Some(BuzzerInteractiveAction::Exit),
    }
}

#[derive(Debug, Subcommand)]
enum PdCommand {
    Pps {
        #[command(subcommand)]
        command: PpsCommand,
    },
}

#[derive(Debug, Subcommand)]
enum PpsCommand {
    #[command(about = "Set a manual PPS override. Avoid large changes while heating.")]
    Set(PpsSetArgs),
    #[command(about = "Clear the manual PPS override and return to automatic power control.")]
    Clear(TargetSelector),
}

#[derive(Debug, Args)]
struct PpsSetArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(
        long = "volts",
        help = "Manual PPS voltage in volts, using 0.1V steps."
    )]
    volts: String,
    #[arg(
        long = "amps",
        help = "Manual PPS requested current in amps, using 0.05A steps."
    )]
    amps: Option<String>,
}

#[derive(Debug, Args)]
struct RuntimeSetArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "target-temp-c")]
    target_temp_c: Option<i16>,
    #[arg(long = "selected-preset-slot")]
    selected_preset_slot: Option<usize>,
    #[arg(long = "presets-file")]
    presets_file: Option<PathBuf>,
    #[arg(long = "preset-slot")]
    preset_slot: Option<usize>,
    #[arg(long = "preset-temp-c")]
    preset_temp_c: Option<i16>,
    #[arg(long = "preset-disabled")]
    preset_disabled: bool,
    #[arg(long = "active-cooling")]
    active_cooling: Option<bool>,
    #[arg(long = "post-heat-cooling", value_parser = ["off", "normal", "fast"])]
    post_heat_cooling: Option<String>,
    #[arg(long = "heating-fan-guard", value_parser = ["off", "low", "medium", "high"])]
    heating_fan_guard: Option<String>,
    #[arg(long = "heater-enabled")]
    heater_enabled: Option<bool>,
    #[arg(long = "fault-attention-acknowledged")]
    fault_attention_acknowledged: bool,
}

#[derive(Debug, Subcommand)]
enum WifiCommand {
    Set(WifiSetArgs),
    Clear(TargetSelector),
    /// Stop the current WiFi station attempt without erasing saved credentials.
    Cancel(TargetSelector),
}

#[derive(Debug, Subcommand)]
enum CalibrationCommand {
    Get(TargetSelector),
    Capture(CalibrationCaptureArgs),
    Delete(CalibrationDeleteArgs),
    Clear(CalibrationChannelArgs),
    SetSlotFit(CalibrationSetSlotFitArgs),
    SetActiveSlot(CalibrationSetActiveSlotArgs),
    Import(CalibrationImportArgs),
    Export(CalibrationExportArgs),
    Collect(CalibrationCollectArgs),
}

#[derive(Debug, Subcommand)]
enum CalibrationModeCommand {
    Status(TargetSelector),
    Exit(TargetSelector),
    Voltage {
        #[command(subcommand)]
        command: VoltageCalibrationCommand,
    },
    Temperature {
        #[command(subcommand)]
        command: TemperatureCalibrationCommand,
    },
    HeaterCurve {
        #[command(subcommand)]
        command: HeaterCurveCalibrationCommand,
    },
}

#[derive(Debug, Subcommand)]
enum VoltageCalibrationCommand {
    Enter(PpsCalibrationEnterArgs),
    Set(PpsCalibrationSetArgs),
    Step(PpsCalibrationStepArgs),
    Capture(VoltageCalibrationCaptureArgs),
    Auto(TargetSelector),
    Job {
        #[command(subcommand)]
        command: CalibrationJobCommand,
    },
}

#[derive(Debug, Subcommand)]
enum TemperatureCalibrationCommand {
    Enter(TemperatureCalibrationEnterArgs),
    SetTarget(TemperatureCalibrationTargetArgs),
    Heater(TemperatureCalibrationHeaterArgs),
    Capture(TemperatureCalibrationCaptureArgs),
}

#[derive(Debug, Subcommand)]
enum HeaterCurveCalibrationCommand {
    Enter(PpsCalibrationEnterArgs),
    Set(PpsCalibrationSetArgs),
    Heater(HeaterCurveCalibrationHeaterArgs),
    Job {
        #[command(subcommand)]
        command: CalibrationJobCommand,
    },
}

#[derive(Debug, Subcommand)]
enum CalibrationJobCommand {
    Status(TargetSelector),
    Cancel(TargetSelector),
}

#[derive(Debug, Subcommand)]
enum HeaterCurveCommand {
    Get(TargetSelector),
    Preview(HeaterCurveFileArgs),
    ClearPreview(TargetSelector),
    Save(TargetSelector),
    Export(HeaterCurveFileArgs),
}

#[derive(Debug, Subcommand)]
enum ThermalCommand {
    Model {
        #[command(subcommand)]
        command: ThermalModelCommand,
    },
    Profile {
        #[command(subcommand)]
        command: ThermalProfileCommand,
    },
    SelfTest(ThermalSelfTestArgs),
    #[command(
        name = "tune",
        visible_alias = "flagship-tune",
        about = "Run the owner-facing 5A full-batch thermal tuning workflow and emit a canonical preliminary review bundle."
    )]
    Tune(ThermalFlagshipTuneArgs),
    Report {
        #[command(subcommand)]
        command: ThermalReportCommand,
    },
    #[command(
        about = "Recompute analysis and tuned candidate from an existing thermal self-test run."
    )]
    Retune(ThermalRetuneArgs),
}

#[derive(Debug, Subcommand)]
enum ThermalModelCommand {
    #[command(
        about = "Start one selected-APDO full-voltage transient calibration run to 220C, including heater-curve sampling."
    )]
    Calibrate(TargetSelector),
}

#[derive(Debug, Subcommand)]
enum ThermalReportCommand {
    #[command(
        about = "Render a completed raw thermal self-test as the canonical four-file HTML evidence bundle."
    )]
    RenderSelfTest(ThermalSelfTestReportArgs),
    #[command(
        about = "Rerender a legacy preliminary thermal review bundle into the canonical compliant HTML bundle."
    )]
    RerenderLegacy(ThermalLegacyReportArgs),
}

#[derive(Debug, Subcommand)]
enum ThermalProfileCommand {
    Preview(ThermalProfileFileArgs),
    ClearPreview(TargetSelector),
    Save(ThermalProfileFileArgs),
    ClearSaved(ThermalProfileClearArgs),
}

#[derive(Debug, Args)]
struct ThermalProfileFileArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    file: PathBuf,
    #[arg(long = "profile-mode", value_enum, default_value = "65w")]
    profile_mode: ThermalProfileMode,
}

#[derive(Debug, Args)]
struct ThermalProfileClearArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "profile-mode", value_enum, default_value = "65w")]
    profile_mode: ThermalProfileMode,
}

#[derive(Debug, Args, Clone)]
struct ThermalSelfTestArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(
        long = "source-kind",
        value_enum,
        default_value = "isolapurr",
        help = "Bench source provider used for thermal HIL. The current default is isolapurr."
    )]
    source_kind: BenchSourceKind,
    #[arg(
        long = "source-id",
        alias = "source-device-id",
        help = "Expected bench source identity returned by the selected source provider."
    )]
    source_id: String,
    #[arg(
        long = "source-url",
        help = "Bench source URL used by the selected source provider. The default isolapurr provider uses LAN HTTP only."
    )]
    source_url: String,
    #[arg(long = "profile-mode", value_enum, default_value = "auto")]
    profile_mode: ThermalProfileMode,
    #[arg(
        long = "source-voltage-v",
        help = "Optional low-level source-voltage override."
    )]
    source_voltage_v: Option<String>,
    #[arg(
        long = "source-current-a",
        help = "Optional low-level source-current override."
    )]
    source_current_a: Option<String>,
    #[arg(
        long = "source-power-watts",
        default_value_t = 0,
        help = "Requested bench-source capability power ceiling used for thermal HIL source setup. Defaults to the resolved thermal profile mode/bank ceiling."
    )]
    source_power_watts: u16,
    #[arg(
        long = "source-mode",
        default_value = "auto-follow",
        value_parser = ["auto-follow", "manual-forced"],
        help = "Bench source mode. The default isolapurr provider supports auto-follow or manual-forced."
    )]
    source_mode: String,
    #[arg(long = "sample-interval-ms", default_value_t = 300)]
    sample_interval_ms: u64,
    #[arg(
        long = "evaluation-mode",
        value_enum,
        default_value = "hold-confirm",
        help = "Host-side evaluation mode. tuning-scout keeps source/runtime/sample-rate faults hard, but leaves full-speed/overshoot/p2p as scored diagnostics."
    )]
    evaluation_mode: ThermalSelfTestEvaluationMode,
    #[arg(long = "hold-seconds", default_value_t = 60)]
    hold_seconds: u64,
    #[arg(long = "stage-timeout-seconds", default_value_t = 180)]
    stage_timeout_seconds: u64,
    #[arg(
        long = "warmup-timeout-seconds",
        default_value_t = 180,
        help = "Explicit warmup timeout. It is tracked separately from the overall stage timeout and must not be derived from remaining target budget."
    )]
    warmup_timeout_seconds: u64,
    #[arg(
        long = "runtime-rearm-attempts",
        default_value_t = 1,
        help = "Bounded automatic recovery count for transient sensor faults, guarded temperature observations, and recoverable runtime resets during thermal HIL."
    )]
    runtime_rearm_attempts: u8,
    #[arg(
        long = "calibration-run",
        action = ArgAction::SetTrue,
        help = "Collect the full hold window even when timing acceptance gates fail; safety faults still stop immediately."
    )]
    calibration_run: bool,
    #[arg(
        long = "optimize-targets-c",
        help = "Comma-separated sparse tuning targets. Defaults to a range-covering subset of the validation targets."
    )]
    optimize_targets_c: Option<String>,
    #[arg(
        long = "skip-optimize",
        action = ArgAction::SetTrue,
        help = "Skip the tuning pass and run the validation ladder once with the provided seed profile."
    )]
    skip_optimize: bool,
    #[arg(long = "cooldown-temp-c", default_value_t = 40.0)]
    cooldown_temp_c: f64,
    #[arg(long = "cooldown-timeout-seconds", default_value_t = 7200)]
    cooldown_timeout_seconds: u64,
    #[arg(
        long = "targets-c",
        help = "Comma-separated validation target list. Defaults to 60,140,220 during development runs. Supported values are 60,80,100,120,140,160,180,200,220,240,250."
    )]
    targets_c: Option<String>,
    #[arg(long = "seed-profile-file")]
    seed_profile_file: Option<PathBuf>,
    #[arg(
        long = "candidate-profile-file",
        action = ArgAction::Append,
        help = "Repeat for batch comparison of multiple profiles at one target. Batch runs never save EEPROM."
    )]
    candidate_profile_files: Vec<PathBuf>,
    #[arg(long = "output-dir", default_value = "thermal-self-test-runs")]
    output_dir: PathBuf,
    #[arg(long = "dry-run", action = ArgAction::SetTrue)]
    dry_run: bool,
    // Internal flagship-tuning deadline. The public self-test command remains unbounded
    // except for its explicit stage and cooldown timeouts.
    #[arg(skip)]
    execution_deadline: Option<StdInstant>,
}

#[derive(Debug, Clone)]
struct ThermalSourceSelection {
    resolved_bank: &'static str,
    detected_source_class: &'static str,
    detected_source_class_basis: &'static str,
    default_voltage_mv: u16,
    default_current_ma: u16,
}

#[derive(Debug, Args, Clone)]
struct ThermalRetuneArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "run-dir")]
    run_dir: PathBuf,
    #[arg(
        long = "optimize-targets-c",
        help = "Optional override for the sparse tuning targets used during replay."
    )]
    optimize_targets_c: Option<String>,
    #[arg(
        long = "apply-preview",
        action = ArgAction::SetTrue,
        help = "Apply the replayed candidate as a RAM-only thermal profile preview after artifacts are written."
    )]
    apply_preview: bool,
}

#[derive(Debug, Args, Clone)]
struct ThermalFlagshipTuneArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(
        long = "source-kind",
        value_enum,
        default_value = "isolapurr",
        help = "Bench source provider used for flagship thermal HIL."
    )]
    source_kind: BenchSourceKind,
    #[arg(
        long = "source-id",
        alias = "source-device-id",
        help = "Expected bench source identity returned by the selected source provider."
    )]
    source_id: String,
    #[arg(
        long = "source-url",
        help = "Bench source URL used by the selected source provider."
    )]
    source_url: String,
    #[arg(long = "profile-mode", value_enum, default_value = "100w")]
    profile_mode: ThermalProfileMode,
    #[arg(
        long = "source-voltage-v",
        help = "Optional low-level source-voltage override."
    )]
    source_voltage_v: Option<String>,
    #[arg(
        long = "source-current-a",
        help = "Optional low-level source-current override."
    )]
    source_current_a: Option<String>,
    #[arg(
        long = "source-power-watts",
        help = "Requested bench-source capability power ceiling used for flagship thermal HIL source setup."
    )]
    source_power_watts: Option<u16>,
    #[arg(
        long = "source-mode",
        default_value = "auto-follow",
        value_parser = ["auto-follow", "manual-forced"],
        help = "Bench source mode."
    )]
    source_mode: String,
    #[arg(long = "sample-interval-ms", default_value_t = 300)]
    sample_interval_ms: u64,
    #[arg(
        long = "runtime-rearm-attempts",
        default_value_t = 3,
        help = "Bounded automatic recovery count for transient sensor faults and recoverable runtime resets during flagship thermal HIL."
    )]
    runtime_rearm_attempts: u8,
    #[arg(
        long = "anchor-targets-c",
        default_value = "60,80,100,120,140,160,180,220,240",
        help = "Deprecated legacy flag. Canonical 5A full-batch tuning now uses a single same-grade target set."
    )]
    anchor_targets_c: String,
    #[arg(
        long = "validation-targets-c",
        default_value = "60,80,100,120,140,160,180,220,240",
        help = "Deprecated legacy flag. Canonical 5A full-batch tuning no longer runs a separate validation tier."
    )]
    validation_targets_c: String,
    #[arg(
        long = "tune-targets-c",
        default_value = "60,80,100,120,140,160,180,220,240",
        help = "Comma-separated full-batch tuning target set. Execution order is derived recursively from the physical temperature order."
    )]
    tune_targets_c: String,
    #[arg(
        long = "seed-profile-file",
        help = "Optional starting sparse/full thermal profile. Defaults to the resolved bank seed path."
    )]
    seed_profile_file: Option<PathBuf>,
    #[arg(
        long = "output-root",
        default_value = "thermal-self-test-runs",
        help = "Root directory for flagship tuning artifacts."
    )]
    output_root: PathBuf,
    #[arg(
        long = "bundle-dir",
        help = "Output directory for the canonical owner-facing preliminary review bundle."
    )]
    bundle_dir: Option<PathBuf>,
    #[arg(long = "per-target-budget-seconds", default_value_t = 1_200)]
    per_target_budget_seconds: u64,
    #[arg(
        long = "max-tuning-rounds",
        help = "Optional debug-only round cap. Omit to tune until the per-target budget is exhausted."
    )]
    max_tuning_rounds: Option<u32>,
    #[arg(long = "scout-hold-seconds", default_value_t = 12)]
    scout_hold_seconds: u64,
    #[arg(long = "confirm-hold-seconds", default_value_t = 60)]
    confirm_hold_seconds: u64,
    #[arg(long = "dry-run", action = ArgAction::SetTrue)]
    dry_run: bool,
}

#[derive(Debug, Args, Clone)]
struct ThermalLegacyReportArgs {
    #[arg(
        long = "legacy-bundle-dir",
        help = "Directory containing legacy run.bundle.json / samples.ndjson / thermal-profile.accepted.json."
    )]
    legacy_bundle_dir: PathBuf,
    #[arg(
        long = "output-dir",
        help = "Output directory for the rerendered compliant bundle. Defaults to <legacy-bundle-dir>-rerendered."
    )]
    output_dir: Option<PathBuf>,
}

#[derive(Debug, Args, Clone)]
struct ThermalSelfTestReportArgs {
    #[arg(
        long = "run-dir",
        help = "Directory containing a completed thermal self-test run.json and samples.ndjson."
    )]
    run_dir: Vec<PathBuf>,
    #[arg(
        long = "output-dir",
        help = "Output directory for the canonical HTML bundle. Defaults to a sibling <run-dir>-html-report directory."
    )]
    output_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct HeaterCurveFileArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    file: PathBuf,
}

#[derive(Debug, Args)]
struct CalibrationChannelArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    channel: String,
}

#[derive(Debug, Args)]
struct CalibrationCaptureArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    channel: String,
    #[arg(long = "reference-temp-c")]
    reference_temp_c: Option<f32>,
    #[arg(long = "reference-vin-volts")]
    reference_vin_volts: Option<String>,
    #[arg(long = "reference-vin-mv")]
    reference_vin_mv: Option<u32>,
    #[arg(long = "observed-mv")]
    observed_mv: Option<u16>,
    #[arg(long = "expected-mv")]
    expected_mv: Option<u16>,
}

#[derive(Debug, Args)]
struct CalibrationDeleteArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    channel: String,
    #[arg(long = "sample-index")]
    sample_index: usize,
}

#[derive(Debug, Args)]
struct CalibrationSetSlotFitArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    channel: String,
    #[arg(long)]
    slot: String,
    #[arg(long)]
    gain: f32,
    #[arg(long = "offset-mv")]
    offset_mv: f32,
}

#[derive(Debug, Args)]
struct CalibrationSetActiveSlotArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    channel: String,
    #[arg(long)]
    slot: String,
}

#[derive(Debug, Args)]
struct CalibrationImportArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    file: PathBuf,
}

#[derive(Debug, Args)]
struct CalibrationExportArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    file: PathBuf,
}

#[derive(Debug, Args)]
struct CalibrationCollectArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(
        long = "source-current-a",
        alias = "current-a",
        help = "External bench source current in amps, using decimal notation."
    )]
    source_current_a: String,
    #[arg(
        long = "source-device-id",
        default_value = "856a14",
        help = "External bench source device id recorded in the output package."
    )]
    source_device_id: String,
    #[arg(
        long = "target-temp-c",
        default_value_t = 270,
        help = "Heater target temperature used to avoid hold logic during capture."
    )]
    target_temp_c: i16,
    #[arg(
        long = "stop-temp-c",
        default_value_t = 250.0,
        help = "Temperature at which the script automatically disables heating."
    )]
    stop_temp_c: f32,
    #[arg(
        long = "sample-interval-ms",
        default_value_t = 500,
        help = "Polling interval for status capture."
    )]
    sample_interval_ms: u64,
    #[arg(
        long = "max-runtime-seconds",
        default_value_t = 3600,
        help = "Safety timeout for a single capture run."
    )]
    max_runtime_seconds: u64,
    #[arg(
        long = "output-dir",
        default_value = "calibration-runs",
        help = "Directory where the raw and derived run artifacts are written."
    )]
    output_dir: PathBuf,
    #[arg(long = "dry-run", action = ArgAction::SetTrue)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct PpsCalibrationEnterArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "volts")]
    volts: Option<String>,
    #[arg(long = "heater-enabled")]
    heater_enabled: Option<bool>,
}

#[derive(Debug, Args)]
struct PpsCalibrationSetArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "volts")]
    volts: String,
}

#[derive(Debug, Args)]
struct PpsCalibrationStepArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "delta-v", default_value_t = 1)]
    delta_v: i16,
}

#[derive(Debug, Args)]
struct VoltageCalibrationCaptureArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "volts")]
    volts: Option<String>,
    #[arg(long = "millivolts")]
    millivolts: Option<u32>,
}

#[derive(Debug, Args)]
struct TemperatureCalibrationEnterArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "target-adc-mv")]
    target_adc_mv: Option<u16>,
    #[arg(long = "volts")]
    volts: Option<String>,
    #[arg(long = "heater-enabled")]
    heater_enabled: Option<bool>,
}

#[derive(Debug, Args)]
struct TemperatureCalibrationTargetArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "target-adc-mv")]
    target_adc_mv: u16,
}

#[derive(Debug, Args)]
struct TemperatureCalibrationHeaterArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "enabled", action = ArgAction::Set, value_parser = clap::value_parser!(bool))]
    enabled: bool,
}

#[derive(Debug, Args)]
struct TemperatureCalibrationCaptureArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "reference-temp-c")]
    reference_temp_c: f32,
    #[arg(long = "observed-mv")]
    observed_mv: Option<u16>,
}

#[derive(Debug, Args)]
struct HeaterCurveCalibrationHeaterArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "enabled", action = ArgAction::Set, value_parser = clap::value_parser!(bool))]
    enabled: bool,
}

#[derive(Debug, Args)]
struct WifiSetArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    ssid: String,
    #[arg(long)]
    password: Option<String>,
    #[arg(long = "static-ip")]
    static_ip: Option<Ipv4Addr>,
    #[arg(long = "static-prefix-len")]
    static_prefix_len: Option<u8>,
    #[arg(long = "static-gateway")]
    static_gateway: Option<Ipv4Addr>,
    #[arg(long = "static-dns")]
    static_dns: Option<Ipv4Addr>,
    #[arg(long = "telemetry-interval-ms")]
    telemetry_interval_ms: Option<u32>,
}

#[derive(Debug, Args)]
struct UpdateArgs {
    #[arg(long, value_name = "SERIAL_PORT")]
    port: String,
    #[arg(long, value_name = "BUNDLE")]
    bundle: PathBuf,
}

#[derive(Debug, Args)]
struct FlashArgs {
    #[arg(long, value_name = "SERIAL_PORT")]
    port: String,
    #[arg(long, value_name = "ELF")]
    elf: Option<PathBuf>,
    #[arg(
        long,
        help = "Skip the Developer EEPROM backup when paired with --confirm NO_EEPROM_BACKUP; no ROM-mode precondition applies"
    )]
    skip_backup: bool,
    #[arg(
        long,
        help = "Literal confirmation required by --skip-backup: NO_EEPROM_BACKUP"
    )]
    confirm: Option<String>,
}

#[derive(Debug, Args)]
struct RecoverArgs {
    #[arg(long, value_name = "SERIAL_PORT")]
    port: String,
    #[arg(long, value_name = "ELF")]
    elf: PathBuf,
    #[arg(long)]
    confirm: String,
}

#[derive(Debug, Args)]
struct MonitorArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long, default_value_t = 20)]
    tail: usize,
}

#[derive(Debug, Subcommand)]
enum HardwareCommand {
    Available,
    Recent,
    List,
    Path,
    Save {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        device: String,
        #[arg(long)]
        devd: Option<String>,
    },
    Forget {
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum UsbPortCommand {
    Set { port: String },
    Show,
}

#[derive(Debug, Subcommand)]
enum EepromCommand {
    Export(EepromExportArgs),
    Import(EepromImportArgs),
    Erase(EepromEraseArgs),
}

#[derive(Debug, Args)]
struct EepromExportArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct EepromImportArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    input: PathBuf,
}

#[derive(Debug, Args)]
struct EepromEraseArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long, default_value = "ERASE EEPROM")]
    confirm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SavedTransport {
    Usb,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SavedHardware {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    transport: SavedTransport,
    device: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    devd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_seen_unix_seconds: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HardwareRegistry {
    #[serde(default = "hardware_registry_schema_version")]
    schema_version: u8,
    #[serde(default)]
    hardware: Vec<SavedHardware>,
}

#[derive(Debug, Clone)]
struct ResolvedUsbTarget {
    device: String,
    devd: String,
    hardware_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Lease {
    lease_id: String,
    ttl_ms: u64,
}

impl Default for HardwareRegistry {
    fn default() -> Self {
        Self {
            schema_version: hardware_registry_schema_version(),
            hardware: Vec::new(),
        }
    }
}

fn static_ipv4_value(
    address: Option<Ipv4Addr>,
    prefix_len: Option<u8>,
    gateway: Option<Ipv4Addr>,
    dns: Option<Ipv4Addr>,
) -> Result<Option<Value>, io::Error> {
    let Some(address) = address else {
        if prefix_len.is_some() || gateway.is_some() || dns.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--static-prefix-len, --static-gateway, and --static-dns require --static-ip",
            ));
        }
        return Ok(None);
    };
    let prefix_len = prefix_len.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "--static-ip requires --static-prefix-len",
        )
    })?;
    let gateway = gateway.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "--static-ip requires --static-gateway",
        )
    })?;
    let dns = dns.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "--static-ip requires --static-dns",
        )
    })?;
    if prefix_len > 32
        || !is_unicast_static_ipv4(address)
        || !is_unicast_static_ipv4(gateway)
        || !is_unicast_static_ipv4(dns)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--static-ip must be a unicast IPv4 address with a prefix length from 0 through 32",
        ));
    }
    Ok(Some(json!({
        "address": address.octets(),
        "prefixLen": prefix_len,
        "gateway": gateway.octets(),
        "dns": dns.octets(),
    })))
}

fn wifi_set_body(
    ssid: String,
    password: Option<String>,
    static_ipv4: Option<Value>,
    telemetry_interval_ms: Option<u32>,
) -> Value {
    let mut body = serde_json::Map::from_iter([
        ("op".to_string(), json!(WifiConfigOp::Set)),
        ("ssid".to_string(), json!(ssid)),
    ]);
    if let Some(password) = password {
        body.insert("password".to_string(), json!(password));
    }
    if let Some(static_ipv4) = static_ipv4 {
        body.insert("staticIpv4".to_string(), static_ipv4);
    }
    if let Some(interval) = telemetry_interval_ms {
        body.insert("telemetryIntervalMs".to_string(), json!(interval));
    }
    Value::Object(body)
}

fn is_unicast_static_ipv4(address: Ipv4Addr) -> bool {
    let first = address.octets()[0];
    first != 0 && first != 127 && first < 224
}
