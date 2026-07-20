use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
    time::{Duration, Instant as StdInstant, SystemTime, UNIX_EPOCH},
};

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use flux_purr_devd::{
    DEFAULT_DEVD_URL, FirmwareArtifact, FirmwareArtifactCatalog, WifiConfigOp,
    hardware_registry_path, read_user_config, write_user_config,
};
use reqwest::{Client, Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[path = "flux_purr/thermal_retune.rs"]
mod thermal_retune;
#[path = "flux_purr/thermal_report.rs"]
mod thermal_report;

#[derive(Debug, Parser)]
#[command(name = "flux-purr")]
#[command(about = "Flux Purr CLI for USB/devd hardware workflows")]
struct Cli {
    #[arg(long, default_value = DEFAULT_DEVD_URL)]
    devd: String,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Devices,
    Identity(TargetSelector),
    Status(TargetSelector),
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
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
    Flash(FlashArgs),
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
}

#[derive(Debug, Subcommand)]
enum RuntimeCommand {
    Get(TargetSelector),
    Set(RuntimeSetArgs),
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
    #[arg(long = "heater-enabled")]
    heater_enabled: Option<bool>,
    #[arg(long = "fault-attention-acknowledged")]
    fault_attention_acknowledged: bool,
}

#[derive(Debug, Subcommand)]
enum WifiCommand {
    Set(WifiSetArgs),
    Clear(TargetSelector),
}

#[derive(Debug, Subcommand)]
enum CalibrationCommand {
    Get(TargetSelector),
    Capture(CalibrationCaptureArgs),
    Delete(CalibrationDeleteArgs),
    Clear(CalibrationChannelArgs),
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
    Auto(TargetSelector),
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
    Profile {
        #[command(subcommand)]
        command: ThermalProfileCommand,
    },
    SelfTest(ThermalSelfTestArgs),
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
enum ThermalReportCommand {
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
        default_value_t = THERMAL_SOURCE_65W_POWER_WATTS as u16,
        help = "Requested bench-source capability power ceiling used for thermal HIL source setup."
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
        help = "Bounded automatic recovery count for transient sensor faults and recoverable runtime resets during thermal HIL."
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
    #[arg(long = "auto-reconnect")]
    auto_reconnect: Option<bool>,
    #[arg(long = "telemetry-interval-ms")]
    telemetry_interval_ms: Option<u32>,
}

#[derive(Debug, Args)]
struct FlashArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long = "artifact-id")]
    artifact_id: Option<String>,
    #[arg(long = "manifest-path")]
    manifest_path: Option<PathBuf>,
    #[arg(long = "no-dry-run", default_value_t = true, action = ArgAction::SetFalse)]
    dry_run: bool,
    #[arg(long)]
    confirm: Option<String>,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();
    let client = Client::new();
    let payload = match cli.command {
        Command::Devices => {
            request_json(&client, Method::GET, &cli.devd, "/api/v1/devices", None).await?
        }
        Command::Identity(selector) => {
            request_with_lease(
                &client,
                resolve_target(selector, &cli.devd)?,
                Method::GET,
                "/identity",
                None,
            )
            .await?
        }
        Command::Status(selector) => {
            request_with_lease(
                &client,
                resolve_target(selector, &cli.devd)?,
                Method::GET,
                "/status",
                None,
            )
            .await?
        }
        Command::Runtime { command } => match command {
            RuntimeCommand::Get(selector) => {
                request_with_lease(
                    &client,
                    resolve_target(selector, &cli.devd)?,
                    Method::GET,
                    "/status",
                    None,
                )
                .await?
            }
            RuntimeCommand::Set(args) => {
                let resolved = resolve_target(args.target.clone(), &cli.devd)?;
                let body = runtime_body(&client, &resolved, args).await?;
                request_with_lease(&client, resolved, Method::PUT, "/runtime", Some(body)).await?
            }
        },
        Command::Pd { command } => match command {
            PdCommand::Pps { command } => match command {
                PpsCommand::Set(args) => {
                    let millivolts = parse_pps_volts(&args.volts)?;
                    let mut body = json!({
                        "manualPpsEnabled": true,
                        "manualPpsMv": millivolts,
                    });
                    if let Some(amps) = &args.amps {
                        body["manualPpsMa"] = json!(parse_pps_amps(amps)?);
                    }
                    request_with_lease(
                        &client,
                        resolve_target(args.target.clone(), &cli.devd)?,
                        Method::PUT,
                        "/runtime",
                        Some(body),
                    )
                    .await?
                }
                PpsCommand::Clear(selector) => {
                    let body = json!({"manualPpsEnabled": false});
                    request_with_lease(
                        &client,
                        resolve_target(selector, &cli.devd)?,
                        Method::PUT,
                        "/runtime",
                        Some(body),
                    )
                    .await?
                }
            },
        },
        Command::Wifi { command } => match command {
            WifiCommand::Set(args) => {
                let resolved = resolve_target(args.target.clone(), &cli.devd)?;
                let body = json!({
                    "op": WifiConfigOp::Set,
                    "ssid": args.ssid,
                    "password": args.password,
                    "autoReconnect": args.auto_reconnect,
                    "telemetryIntervalMs": args.telemetry_interval_ms,
                });
                request_with_lease(&client, resolved, Method::PUT, "/wifi", Some(body)).await?
            }
            WifiCommand::Clear(selector) => {
                let body = json!({"op": WifiConfigOp::Clear});
                request_with_lease(
                    &client,
                    resolve_target(selector, &cli.devd)?,
                    Method::PUT,
                    "/wifi",
                    Some(body),
                )
                .await?
            }
        },
        Command::Calibration { command } => {
            handle_calibration_command(&client, &cli.devd, command).await?
        }
        Command::CalibrationMode { command } => {
            handle_calibration_mode_command(&client, &cli.devd, command).await?
        }
        Command::HeaterCurve { command } => {
            handle_heater_curve_command(&client, &cli.devd, command).await?
        }
        Command::Thermal { command } => handle_thermal_command(&client, &cli.devd, command).await?,
        Command::Flash(args) => {
            let resolved = resolve_target(args.target.clone(), &cli.devd)?;
            let artifact = resolve_artifact(
                &client,
                &resolved.devd,
                args.manifest_path.as_deref(),
                args.artifact_id.as_deref(),
            )
            .await?;
            flash_with_lease(&client, resolved, artifact, args.dry_run, args.confirm).await?
        }
        Command::Monitor(args) => {
            monitor_once(
                &client,
                resolve_target(args.target.clone(), &cli.devd)?,
                args.tail,
            )
            .await?
        }
        Command::Hardware { command } => {
            handle_hardware_command(&client, &cli.devd, command).await?
        }
        Command::UsbPort { command } => handle_usb_port_command(command)?,
    };

    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&redact_cli_sensitive(&payload))?
        );
    } else {
        println!("{}", render_human(&payload)?);
    }
    Ok(())
}

async fn request_json(
    client: &Client,
    method: Method,
    base: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut request = client.request(method, api_url(base, path)?);
    if let Some(body) = body {
        request = request.json(&body);
    }
    response_json_or_error(request.send().await?).await
}

async fn request_with_lease(
    client: &Client,
    resolved: ResolvedUsbTarget,
    method: Method,
    suffix: &str,
    body: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let result = request_leased(client, &resolved, &lease.lease_id, method, suffix, body).await;
    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();
    let value = result?;
    if let Some(id) = resolved.hardware_id.as_deref() {
        let _ = remember_usb(id, &resolved.device, &resolved.devd);
    }
    Ok(value)
}

async fn flash_with_lease(
    client: &Client,
    resolved: ResolvedUsbTarget,
    artifact: FirmwareArtifact,
    dry_run: bool,
    confirm: Option<String>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    if dry_run {
        let body = json!({
            "artifact": artifact,
            "dryRun": true,
            "confirm": confirm,
        });
        return request_with_lease(client, resolved, Method::POST, "/flash", Some(body)).await;
    }

    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let dry_run_body = json!({
        "artifact": artifact.clone(),
        "dryRun": true,
    });
    let dry_run_result = request_leased(
        client,
        &resolved,
        &lease.lease_id,
        Method::POST,
        "/flash",
        Some(dry_run_body),
    )
    .await;
    let value = match dry_run_result {
        Ok(_) => {
            let flash_body = json!({
                "artifact": artifact,
                "dryRun": false,
                "confirm": confirm,
            });
            request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::POST,
                "/flash",
                Some(flash_body),
            )
            .await
        }
        Err(error) => Err(error),
    };
    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();
    let payload = value?;
    if let Some(id) = resolved.hardware_id.as_deref() {
        let _ = remember_usb(id, &resolved.device, &resolved.devd);
    }
    Ok(payload)
}

async fn request_leased(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    method: Method,
    suffix: &str,
    body: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let path = format!(
        "/api/v1/devices/{}{}",
        encode_path_segment(&resolved.device),
        suffix
    );
    let mut url = api_url(&resolved.devd, &path)?;
    match method {
        Method::GET | Method::POST if body.is_none() => {
            url.query_pairs_mut().append_pair("lease_id", lease_id);
        }
        _ => {}
    }
    let mut request = client.request(method, url);
    if let Some(mut body) = body {
        if let Some(object) = body.as_object_mut() {
            object.insert("leaseId".to_string(), Value::String(lease_id.to_string()));
        }
        request = request.json(&body);
    }
    response_json_or_error(request.send().await?).await
}

async fn request_thermal_status_with_retry(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    request_thermal_status_with_retry_config(
        client,
        resolved,
        lease_id,
        Duration::from_millis(THERMAL_STATUS_REQUEST_TIMEOUT_MS),
        THERMAL_STATUS_REQUEST_RETRY_ATTEMPTS,
    )
    .await
}

async fn request_thermal_status_with_retry_config(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    timeout: Duration,
    max_attempts: usize,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let attempts = max_attempts.max(1);
    let timeout_ms = timeout.as_millis();
    let mut last_error = "thermal status read did not start".to_string();
    for attempt in 0..attempts {
        match tokio::time::timeout(
            timeout,
            request_leased(client, resolved, lease_id, Method::GET, "/status", None),
        )
        .await
        {
            Ok(Ok(status)) => return Ok(status),
            Ok(Err(error)) => last_error = error.to_string(),
            Err(_) => {
                last_error = format!("thermal /status timed out after {timeout_ms}ms");
            }
        }
        if attempt + 1 < attempts {
            tokio::time::sleep(Duration::from_millis(
                THERMAL_STATUS_REQUEST_RETRY_BACKOFF_MS * (attempt as u64 + 1),
            ))
            .await;
        }
    }
    Err(format!("thermal /status failed after {attempts} attempt(s): {last_error}").into())
}

async fn response_json_or_error(
    response: reqwest::Response,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(format!("HTTP {status} body={body}").into());
    }
    Ok(serde_json::from_str(&body)?)
}

async fn handle_calibration_command(
    client: &Client,
    default_devd: &str,
    command: CalibrationCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        CalibrationCommand::Get(selector) => {
            request_with_lease(
                client,
                resolve_target(selector, default_devd)?,
                Method::GET,
                "/calibration",
                None,
            )
            .await
        }
        CalibrationCommand::Capture(args) => {
            let mut body = serde_json::Map::new();
            body.insert("op".to_string(), json!("capture"));
            body.insert(
                "channel".to_string(),
                json!(parse_calibration_channel(&args.channel)?),
            );
            insert_if_some(&mut body, "referenceTempC", args.reference_temp_c);
            insert_if_some(
                &mut body,
                "referenceVinMv",
                parse_reference_vin_mv(args.reference_vin_mv, args.reference_vin_volts.as_deref())?,
            );
            insert_if_some(&mut body, "observedMv", args.observed_mv);
            insert_if_some(&mut body, "expectedMv", args.expected_mv);
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/calibration",
                Some(Value::Object(body)),
            )
            .await
        }
        CalibrationCommand::Delete(args) => {
            let body = json!({
                "op": "delete",
                "channel": parse_calibration_channel(&args.channel)?,
                "sampleIndex": args.sample_index,
            });
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/calibration",
                Some(body),
            )
            .await
        }
        CalibrationCommand::Clear(args) => {
            let body = json!({
                "op": "clear",
                "channel": parse_calibration_channel(&args.channel)?,
            });
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/calibration",
                Some(body),
            )
            .await
        }
        CalibrationCommand::Import(args) => {
            let imported: Value = serde_json::from_slice(&fs::read(&args.file)?)?;
            let body = json!({
                "op": "import",
                "state": imported,
            });
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/calibration",
                Some(body),
            )
            .await
        }
        CalibrationCommand::Export(args) => {
            let payload = request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::GET,
                "/calibration",
                None,
            )
            .await?;
            if let Some(parent) = args
                .file
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.file, serde_json::to_vec_pretty(&payload)?)?;
            Ok(json!({
                "ok": true,
                "path": args.file,
            }))
        }
        CalibrationCommand::Collect(args) => {
            collect_calibration_run(client, default_devd, args).await
        }
    }
}

async fn handle_calibration_mode_command(
    client: &Client,
    default_devd: &str,
    command: CalibrationModeCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        CalibrationModeCommand::Status(selector) => {
            let payload = request_with_lease(
                client,
                resolve_target(selector, default_devd)?,
                Method::GET,
                "/status",
                None,
            )
            .await?;
            Ok(payload.get("calibration").cloned().unwrap_or(Value::Null))
        }
        CalibrationModeCommand::Exit(selector) => {
            request_with_lease(
                client,
                resolve_target(selector, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "calibration": {
                        "mode": "off",
                        "ppsEnabled": false,
                        "heaterEnabled": false
                    }
                })),
            )
            .await
        }
        CalibrationModeCommand::Voltage { command } => {
            handle_voltage_calibration_command(client, default_devd, command).await
        }
        CalibrationModeCommand::Temperature { command } => {
            handle_temperature_calibration_command(client, default_devd, command).await
        }
        CalibrationModeCommand::HeaterCurve { command } => {
            handle_heater_curve_calibration_command(client, default_devd, command).await
        }
    }
}

async fn handle_voltage_calibration_command(
    client: &Client,
    default_devd: &str,
    command: VoltageCalibrationCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        VoltageCalibrationCommand::Enter(args) => {
            let calibration =
                calibration_pps_payload("vin_adc", args.volts.as_deref(), args.heater_enabled)?;
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({ "calibration": calibration })),
            )
            .await
        }
        VoltageCalibrationCommand::Set(args) => {
            let calibration = calibration_pps_payload_partial(args.volts.as_str())?;
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({ "calibration": calibration })),
            )
            .await
        }
        VoltageCalibrationCommand::Step(args) => {
            let resolved = resolve_target(args.target, default_devd)?;
            let status =
                request_with_lease(client, resolved.clone(), Method::GET, "/status", None).await?;
            let current_mv = status
                .get("calibration")
                .and_then(|value| value.get("ppsMv"))
                .and_then(Value::as_u64)
                .or_else(|| status.get("manualPpsMv").and_then(Value::as_u64))
                .ok_or("calibration PPS voltage is unavailable")?;
            let next_mv = stepped_pps_mv(current_mv as u16, args.delta_v)?;
            request_with_lease(
                client,
                resolved,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "calibration": {
                        "ppsEnabled": true,
                        "ppsMv": next_mv
                    }
                })),
            )
            .await
        }
        VoltageCalibrationCommand::Capture(args) => {
            let reference_vin_mv = parse_reference_vin_mv(args.millivolts, args.volts.as_deref())?
                .ok_or("voltage capture requires --volts or --millivolts")?;
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/calibration",
                Some(json!({
                    "op": "capture",
                    "channel": "vin_adc",
                    "referenceVinMv": reference_vin_mv
                })),
            )
            .await
        }
        VoltageCalibrationCommand::Auto(target) => {
            request_with_lease(
                client,
                resolve_target(target, default_devd)?,
                Method::POST,
                "/calibration/job",
                Some(json!({
                    "op": "start",
                    "kind": "vin_adc_auto"
                })),
            )
            .await
        }
        VoltageCalibrationCommand::Job { command } => {
            handle_calibration_job_command(client, default_devd, command).await
        }
    }
}

async fn handle_temperature_calibration_command(
    client: &Client,
    default_devd: &str,
    command: TemperatureCalibrationCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        TemperatureCalibrationCommand::Enter(args) => {
            let mut calibration =
                calibration_pps_payload("rtd_adc", args.volts.as_deref(), args.heater_enabled)?;
            if let Some(target_adc_mv) = args.target_adc_mv {
                calibration["targetAdcMv"] = json!(target_adc_mv);
            }
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({ "calibration": calibration })),
            )
            .await
        }
        TemperatureCalibrationCommand::SetTarget(args) => {
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "calibration": {
                        "targetAdcMv": args.target_adc_mv
                    }
                })),
            )
            .await
        }
        TemperatureCalibrationCommand::Heater(args) => {
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "calibration": {
                        "heaterEnabled": args.enabled
                    }
                })),
            )
            .await
        }
        TemperatureCalibrationCommand::Capture(args) => {
            let mut body = serde_json::Map::new();
            body.insert("op".to_string(), json!("capture"));
            body.insert("channel".to_string(), json!("rtd_adc"));
            body.insert("referenceTempC".to_string(), json!(args.reference_temp_c));
            insert_if_some(&mut body, "observedMv", args.observed_mv);
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/calibration",
                Some(Value::Object(body)),
            )
            .await
        }
    }
}

async fn handle_heater_curve_calibration_command(
    client: &Client,
    default_devd: &str,
    command: HeaterCurveCalibrationCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        HeaterCurveCalibrationCommand::Enter(args) => {
            let calibration = calibration_pps_payload(
                "heater_curve",
                args.volts.as_deref(),
                args.heater_enabled,
            )?;
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({ "calibration": calibration })),
            )
            .await
        }
        HeaterCurveCalibrationCommand::Set(args) => {
            let calibration = calibration_pps_payload_partial(args.volts.as_str())?;
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({ "calibration": calibration })),
            )
            .await
        }
        HeaterCurveCalibrationCommand::Heater(args) => {
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "calibration": {
                        "heaterEnabled": args.enabled
                    }
                })),
            )
            .await
        }
        HeaterCurveCalibrationCommand::Auto(target) => {
            request_with_lease(
                client,
                resolve_target(target, default_devd)?,
                Method::POST,
                "/calibration/job",
                Some(json!({
                    "op": "start",
                    "kind": "heater_curve_auto"
                })),
            )
            .await
        }
        HeaterCurveCalibrationCommand::Job { command } => {
            handle_calibration_job_command(client, default_devd, command).await
        }
    }
}

async fn handle_calibration_job_command(
    client: &Client,
    default_devd: &str,
    command: CalibrationJobCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        CalibrationJobCommand::Status(target) => {
            request_with_lease(
                client,
                resolve_target(target, default_devd)?,
                Method::GET,
                "/calibration/job",
                None,
            )
            .await
        }
        CalibrationJobCommand::Cancel(target) => {
            request_with_lease(
                client,
                resolve_target(target, default_devd)?,
                Method::POST,
                "/calibration/job",
                Some(json!({ "op": "cancel" })),
            )
            .await
        }
    }
}

fn calibration_pps_payload(
    mode: &'static str,
    volts: Option<&str>,
    heater_enabled: Option<bool>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut payload = calibration_pps_payload_partial_opt(volts)?;
    payload["mode"] = json!(mode);
    if let Some(heater_enabled) = heater_enabled {
        payload["heaterEnabled"] = json!(heater_enabled);
    }
    Ok(payload)
}

fn calibration_pps_payload_partial(
    volts: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    calibration_pps_payload_partial_opt(Some(volts))
}

fn calibration_pps_payload_partial_opt(
    volts: Option<&str>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut payload = serde_json::Map::new();
    if let Some(volts) = volts {
        payload.insert("ppsEnabled".to_string(), json!(true));
        payload.insert("ppsMv".to_string(), json!(parse_pps_volts(volts)?));
    }
    Ok(Value::Object(payload))
}

fn stepped_pps_mv(
    current_mv: u16,
    delta_v: i16,
) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let stepped = i32::from(current_mv) + i32::from(delta_v) * 1_000;
    if !(5_000..=28_000).contains(&stepped) {
        return Err("PPS voltage step must stay within 5V..28V.".into());
    }
    Ok(stepped as u16)
}

async fn handle_heater_curve_command(
    client: &Client,
    default_devd: &str,
    command: HeaterCurveCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        HeaterCurveCommand::Get(selector) => {
            request_with_lease(
                client,
                resolve_target(selector, default_devd)?,
                Method::GET,
                "/heater-curve",
                None,
            )
            .await
        }
        HeaterCurveCommand::Preview(args) => {
            let imported: Value = serde_json::from_slice(&fs::read(&args.file)?)?;
            let package = imported
                .get("active")
                .cloned()
                .or_else(|| imported.get("package").cloned())
                .unwrap_or(imported);
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/heater-curve",
                Some(json!({
                    "op": "preview",
                    "package": package,
                })),
            )
            .await
        }
        HeaterCurveCommand::ClearPreview(selector) => {
            request_with_lease(
                client,
                resolve_target(selector, default_devd)?,
                Method::PUT,
                "/heater-curve",
                Some(json!({
                    "op": "clear_preview",
                })),
            )
            .await
        }
        HeaterCurveCommand::Save(selector) => {
            request_with_lease(
                client,
                resolve_target(selector, default_devd)?,
                Method::POST,
                "/heater-curve/save",
                Some(json!({})),
            )
            .await
        }
        HeaterCurveCommand::Export(args) => {
            let payload = request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::GET,
                "/heater-curve",
                None,
            )
            .await?;
            if let Some(parent) = args
                .file
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.file, serde_json::to_vec_pretty(&payload)?)?;
            Ok(json!({
                "ok": true,
                "path": args.file,
            }))
        }
    }
}

const THERMAL_SUPPORTED_TARGETS_C: [i16; 11] =
    [60, 80, 100, 120, 140, 160, 180, 200, 220, 240, 250];
const THERMAL_PROFILE_ANCHOR_TARGETS_C: [i16; 6] = [60, 100, 140, 180, 220, 250];
const THERMAL_PPS3A_PERSISTED_TARGETS_C: [i16; 6] = [60, 100, 140, 180, 220, 250];
const THERMAL_PPS5A_PERSISTED_TARGETS_C: [i16; 6] = [60, 100, 140, 180, 220, 240];
const THERMAL_SELF_TEST_DEFAULT_TARGETS_C: [i16; 3] = [60, 140, 220];
const THERMAL_CONTROL_PROFILE_MAX_POINTS: usize = 10;
const THERMAL_APPROACH_CURVE_PREFERRED_MS: u64 = 5_000;
const THERMAL_APPROACH_CURVE_LIMIT_MS: u64 = 10_000;
const THERMAL_APPROACH_CURVE_SIGNIFICANT_DEVIATION_C: f64 = 0.5;
const THERMAL_APPROACH_CURVE_CLASS_MARGIN_C: f64 = 0.4;

fn thermal_profile_preview_runtime_body(mode: ThermalProfileMode, profile: Value) -> Value {
    json!({
        "thermalProfileMode": mode.as_str(),
        "thermalControlProfile": {
            "op": "preview",
            "profile": profile,
        }
    })
}

fn expected_thermal_profile_mode_bank<'a>(
    status: &'a Value,
    expected_mode: ThermalProfileMode,
) -> Result<&'a str, Box<dyn std::error::Error + Send + Sync>> {
    match expected_mode {
        ThermalProfileMode::W65 => Ok("pps3a"),
        ThermalProfileMode::W100 => Ok("pps5a"),
        ThermalProfileMode::Auto => status
            .get("thermalProfileResolvedBank")
            .and_then(Value::as_str)
            .ok_or("status missing thermalProfileResolvedBank".into()),
    }
}

async fn resolve_thermal_profile_runtime_bank(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    expected_mode: ThermalProfileMode,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(bank) = expected_mode.explicit_bank() {
        return Ok(bank.to_string());
    }
    let status = request_leased(client, resolved, lease_id, Method::GET, "/status", None).await?;
    Ok(expected_thermal_profile_mode_bank(&status, expected_mode)?.to_string())
}

async fn request_thermal_profile_persist_with_resolved_bank(
    client: &Client,
    resolved: ResolvedUsbTarget,
    profile_mode: ThermalProfileMode,
    op: &'static str,
    profile: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let bank =
        resolve_thermal_profile_runtime_bank(client, &resolved, &lease.lease_id, profile_mode)
            .await?;
    let mut thermal_control_profile = json!({
        "op": op,
        "bank": bank,
    });
    if let Some(profile) = profile
        && let Some(object) = thermal_control_profile.as_object_mut()
    {
        let profile = if op == "save" {
            thermal_candidate_profile_to_value(&thermal_persisted_profile_for_bank(
                &thermal_candidate_profile_from_value(profile),
                &bank,
            ))
        } else {
            profile
        };
        object.insert("profile".to_string(), profile);
    }
    let result = request_leased(
        client,
        &resolved,
        &lease.lease_id,
        Method::PUT,
        "/runtime",
        Some(json!({
            "thermalProfileMode": profile_mode.as_str(),
            "thermalControlProfile": thermal_control_profile,
        })),
    )
    .await;
    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();
    let payload = result?;
    if let Some(id) = resolved.hardware_id.as_deref() {
        let _ = remember_usb(id, &resolved.device, &resolved.devd);
    }
    Ok(payload)
}

async fn handle_thermal_command(
    client: &Client,
    default_devd: &str,
    command: ThermalCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        ThermalCommand::Profile { command } => match command {
            ThermalProfileCommand::Preview(args) => {
                let imported: Value = serde_json::from_slice(&fs::read(&args.file)?)?;
                let profile = thermal_profile_package_from_value(imported);
                request_with_lease(
                    client,
                    resolve_target(args.target, default_devd)?,
                    Method::PUT,
                    "/runtime",
                    Some(thermal_profile_preview_runtime_body(
                        args.profile_mode,
                        profile,
                    )),
                )
                .await
            }
            ThermalProfileCommand::ClearPreview(selector) => {
                request_with_lease(
                    client,
                    resolve_target(selector, default_devd)?,
                    Method::PUT,
                    "/runtime",
                    Some(json!({
                        "thermalControlProfile": {
                            "op": "clear_preview"
                        }
                    })),
                )
                .await
            }
            ThermalProfileCommand::Save(args) => {
                let imported: Value = serde_json::from_slice(&fs::read(&args.file)?)?;
                let profile = thermal_profile_package_from_value(imported);
                request_thermal_profile_persist_with_resolved_bank(
                    client,
                    resolve_target(args.target, default_devd)?,
                    args.profile_mode,
                    "save",
                    Some(profile),
                )
                .await
            }
            ThermalProfileCommand::ClearSaved(args) => {
                request_thermal_profile_persist_with_resolved_bank(
                    client,
                    resolve_target(args.target, default_devd)?,
                    args.profile_mode,
                    "clear_saved",
                    None,
                )
                .await
            }
        },
        ThermalCommand::SelfTest(args) => {
            collect_thermal_self_test(client, default_devd, args).await
        }
        ThermalCommand::Report { command } => match command {
            ThermalReportCommand::RerenderLegacy(args) => {
                thermal_report::rerender_legacy_preliminary_review_bundle(
                    thermal_report::ThermalLegacyReportInput {
                        legacy_bundle_dir: args.legacy_bundle_dir,
                        output_dir: args.output_dir,
                    },
                )
            }
        },
        ThermalCommand::Retune(args) => retune_thermal_self_test_run(client, default_devd, args).await,
    }
}

#[derive(Debug, Clone)]
struct ThermalStageResult {
    target_temp_c: i16,
    rise_time_ms: u64,
    max_overshoot_c: f64,
    hold_peak_to_peak_c: f64,
    sample_count: usize,
    stop_reason: &'static str,
    terminal_runtime_drop_reason: Option<&'static str>,
    analysis: ThermalStageAnalysis,
    guard: ThermalApproachGuardAnalysis,
    full_speed_to_stable: ThermalFullSpeedStableAnalysis,
}

#[derive(Debug, Clone, Default)]
struct ThermalStageAnalysis {
    first_hold_temp_c: Option<f64>,
    first_hold_error_c: Option<f64>,
    residual_heat_after_hold_entry_c: Option<f64>,
    approach_median_output_permille: Option<u16>,
    approach_median_slope_c_per_s: Option<f64>,
    hold_median_output_permille: Option<u16>,
    hold_p90_output_permille: Option<u16>,
    hold_mean_error_c: Option<f64>,
    hold_max_above_target_c: Option<f64>,
    hold_max_below_target_c: Option<f64>,
    approach_curve_fit_basis: Option<&'static str>,
    approach_curve_start_temp_c: Option<f64>,
    approach_curve_fitted_ms: Option<u64>,
    approach_curve_preferred_ms: Option<u64>,
    approach_curve_limit_ms: Option<u64>,
    approach_curve_max_above_c: Option<f64>,
    approach_curve_max_below_c: Option<f64>,
    approach_curve_mean_abs_error_c: Option<f64>,
    approach_curve_oscillation_c: Option<f64>,
    approach_curve_deviation_class: Option<&'static str>,
    approach_curve_tail_uses_half_floor: Option<bool>,
    approach_sample_count: usize,
    hold_sample_count: usize,
}

#[derive(Debug, Clone, Default)]
struct ThermalApproachGuardAnalysis {
    hold_threshold_temp_c: f64,
    approach_started_at_ms: Option<u64>,
    hold_threshold_crossed_at_ms: Option<u64>,
    first_hold_at_ms: Option<u64>,
    warmup_reentered_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
struct ThermalFullSpeedStableAnalysis {
    warmup_exited_at_ms: Option<u64>,
    stable_window_started_at_ms: Option<u64>,
    stable_window_verified_at_ms: Option<u64>,
    settle_time_ms: Option<u64>,
    failure_reason: Option<&'static str>,
}

#[derive(Debug, Clone)]
struct ThermalReplayStageSample {
    elapsed_ms: u64,
    current_temp_c: f64,
    heater_output_percent: u8,
    control_phase: Option<String>,
    control_phase_in_hold: bool,
    source_voltage_mv: Option<u64>,
    source_current_ma: Option<u64>,
    source_power_mw: Option<u64>,
}

#[derive(Debug, Clone)]
struct ThermalStageAnalyzer {
    target_temp_c: f64,
    first_hold_temp_c: Option<f64>,
    last_elapsed_ms: Option<u64>,
    last_temp_c: Option<f64>,
    approach_output_permille: Vec<u16>,
    approach_slope_c_per_s: Vec<f64>,
    hold_output_permille: Vec<u16>,
    hold_error_c: Vec<f64>,
}

#[derive(Debug, Clone, Default)]
struct ThermalSourceWindowAnalysis {
    sample_count: usize,
    voltage_mv: Option<CalibrationSeriesStats>,
    current_ma: Option<CalibrationSeriesStats>,
    power_mw: Option<CalibrationSeriesStats>,
}

#[derive(Debug, Clone)]
struct ThermalStageSourceAnalysisBuilder {
    target_temp_c: f64,
    first_hold_temp_c: Option<f64>,
    approach: ThermalSourceWindowAnalysis,
    hold: ThermalSourceWindowAnalysis,
}

#[derive(Debug, Clone)]
struct ThermalApproachGuardTracker {
    hold_threshold_temp_c: f64,
    approach_started_at_ms: Option<u64>,
    hold_threshold_crossed_at_ms: Option<u64>,
    first_hold_at_ms: Option<u64>,
    warmup_reentered_at_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct ThermalFullSpeedStableTracker {
    target_temp_c: f64,
    warmup_exited_at_ms: Option<u64>,
    stable_window_started_at_ms: Option<u64>,
    stable_window_verified_at_ms: Option<u64>,
    settle_time_ms: Option<u64>,
    failure_reason: Option<&'static str>,
}

#[derive(Debug, Clone)]
struct ThermalSampleRateTracker {
    elapsed_ms: Vec<u64>,
    below_minimum_since_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct ThermalSampleRateObservation {
    interval_ms: Option<u64>,
    rolling_rate_hz: Option<f64>,
    violation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThermalFullSpeedStableObservation {
    Pending,
    Verified,
    Failed(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ThermalCandidateSettings {
    temp_filter_alpha_permille: u16,
    warmup_reenter_centi_c: u16,
    hold_entry_centi_c: u16,
    hold_exit_centi_c: u16,
    hold_on_centi_c: u16,
    hold_off_centi_c: u16,
    overshoot_cutoff_centi_c: u16,
    approach_max_ticks: u16,
    approach_min_power_ratio_permille: u16,
    hold_kp_permille_per_c: u16,
    hold_ki_permille_per_c_tick: u16,
    hold_blend_ticks: u16,
    hold_reheat_power_permille: u16,
    approach_lead_ticks: u16,
    hold_lead_ticks: u16,
    auto_adjustable_working_floor_mv: u16,
    heater_current_reserve_ma: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ThermalCandidatePoint {
    target_temp_c: i16,
    brake_distance_centi_c: u16,
    warmup_power_permille: u16,
    approach_power_permille: u16,
    approach_floor_power_permille: u16,
    approach_damping_exponent_permille: u16,
    approach_tail_window_centi_c: u16,
    hold_power_permille: u16,
    hold_reheat_power_permille: u16,
    hold_entry_centi_c: u16,
    hold_exit_centi_c: u16,
    hold_on_centi_c: u16,
    hold_off_centi_c: u16,
    overshoot_cutoff_centi_c: u16,
    hold_kp_permille_per_c: u16,
    hold_ki_permille_per_c_tick: u16,
    hold_blend_ticks: u16,
    approach_lead_ticks: u16,
    hold_lead_ticks: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThermalCandidateProfile {
    settings: ThermalCandidateSettings,
    points: Vec<ThermalCandidatePoint>,
}

#[derive(Debug, Clone)]
struct BenchSourceLiveTelemetry {
    voltage_mv: u64,
    current_ma: u64,
    power_mw: u64,
    sample_uptime_ms: u64,
    status: String,
}

const THERMAL_SOURCE_65W_POWER_WATTS: u64 = 65;
const THERMAL_SOURCE_100W_POWER_WATTS: u64 = 100;
const THERMAL_SOURCE_MIN_READY_VOLTAGE_MV: u64 = 5_000;
const THERMAL_STATUS_REQUEST_TIMEOUT_MS: u64 = 1_000;
const THERMAL_STATUS_REQUEST_RETRY_ATTEMPTS: usize = 3;
const THERMAL_STATUS_REQUEST_RETRY_BACKOFF_MS: u64 = 100;

struct BenchSourceTelemetrySampler {
    source_kind: BenchSourceKind,
    source_url: String,
    latest: BenchSourceLiveTelemetry,
    latest_sample_seen_at: tokio::time::Instant,
    pending: Option<
        tokio::task::JoinHandle<
            Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>>,
        >,
    >,
}

impl BenchSourceTelemetrySampler {
    const MAX_STALE_DURATION: Duration = Duration::from_secs(2);

    fn new(
        source_kind: BenchSourceKind,
        source_url: &str,
        initial: BenchSourceLiveTelemetry,
    ) -> Self {
        let mut sampler = Self {
            source_kind,
            source_url: source_url.to_string(),
            latest: initial,
            latest_sample_seen_at: tokio::time::Instant::now(),
            pending: None,
        };
        sampler.start_poll();
        sampler
    }

    fn start_poll(&mut self) {
        let source_kind = self.source_kind;
        let source_url = self.source_url.clone();
        self.pending = Some(tokio::task::spawn_blocking(move || {
            read_bench_source_live_telemetry(source_kind, &source_url)
        }));
    }

    async fn refresh(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let telemetry = if let Some(pending) = self.pending.take() {
            pending.await??
        } else {
            let source_kind = self.source_kind;
            let source_url = self.source_url.clone();
            tokio::task::spawn_blocking(move || {
                read_bench_source_live_telemetry(source_kind, &source_url)
            })
            .await??
        };
        self.latest = telemetry;
        self.latest_sample_seen_at = tokio::time::Instant::now();
        self.start_poll();
        Ok(())
    }

    async fn snapshot(
        &mut self,
    ) -> Result<(BenchSourceLiveTelemetry, u64), Box<dyn std::error::Error + Send + Sync>> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.is_finished())
        {
            let telemetry = self.pending.take().unwrap().await??;
            if telemetry.sample_uptime_ms != self.latest.sample_uptime_ms {
                self.latest_sample_seen_at = tokio::time::Instant::now();
            }
            self.latest = telemetry;
            self.start_poll();
        }
        let stale_ms = self
            .latest_sample_seen_at
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        if self.latest_sample_seen_at.elapsed() > Self::MAX_STALE_DURATION {
            return Err(
                format!("isolapurr USB-C telemetry did not advance for {stale_ms}ms").into(),
            );
        }
        Ok((self.latest.clone(), stale_ms))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThermalHoldObservation {
    Warmup,
    Hold,
    Completed,
}

struct ThermalHoldTracker {
    target_temp_c: i16,
    hold_duration: Duration,
    entered_threshold_c: f64,
    stable_band_c: f64,
    first_hold_started_at: Option<tokio::time::Instant>,
    rise_time_ms: Option<u64>,
    min_c: f64,
    max_c: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThermalRuntimeDropReason {
    UptimeReset,
    LatchedFault,
    HeaterDisarmed,
    WrongMode,
    WrongTarget,
}

impl ThermalRuntimeDropReason {
    fn as_str(self) -> &'static str {
        match self {
            ThermalRuntimeDropReason::UptimeReset => "uptime_reset",
            ThermalRuntimeDropReason::LatchedFault => "latched_fault",
            ThermalRuntimeDropReason::HeaterDisarmed => "heater_disarmed",
            ThermalRuntimeDropReason::WrongMode => "wrong_mode",
            ThermalRuntimeDropReason::WrongTarget => "wrong_target",
        }
    }
}

impl ThermalStageAnalyzer {
    fn new(target_temp_c: i16) -> Self {
        Self {
            target_temp_c: f64::from(target_temp_c),
            first_hold_temp_c: None,
            last_elapsed_ms: None,
            last_temp_c: None,
            approach_output_permille: Vec::new(),
            approach_slope_c_per_s: Vec::new(),
            hold_output_permille: Vec::new(),
            hold_error_c: Vec::new(),
        }
    }

    fn observe(
        &mut self,
        current_temp_c: f64,
        heater_output_percent: u8,
        elapsed_ms: u64,
        control_phase_in_hold: bool,
    ) {
        let output_permille = u16::from(heater_output_percent).saturating_mul(10);
        let error_c = self.target_temp_c - current_temp_c;
        let slope_c_per_s = self.last_elapsed_ms.zip(self.last_temp_c).and_then(
            |(last_elapsed_ms, last_temp_c)| {
                let delta_ms = elapsed_ms.saturating_sub(last_elapsed_ms);
                (delta_ms > 0).then_some((current_temp_c - last_temp_c) / delta_ms as f64 * 1_000.0)
            },
        );
        if control_phase_in_hold {
            self.first_hold_temp_c.get_or_insert(current_temp_c);
        }
        let hold_window_started = self.first_hold_temp_c.is_some();
        let near_target_approach_window = !hold_window_started
            && !control_phase_in_hold
            && error_c >= 0.0
            && error_c <= 8.0
            && current_temp_c <= self.target_temp_c + 0.5;

        if near_target_approach_window {
            self.approach_output_permille.push(output_permille);
            if let Some(slope_c_per_s) = slope_c_per_s {
                if slope_c_per_s > 0.05 {
                    self.approach_slope_c_per_s.push(slope_c_per_s);
                }
            }
        }
        if hold_window_started {
            self.hold_output_permille.push(output_permille);
            self.hold_error_c.push(error_c);
        }

        self.last_elapsed_ms = Some(elapsed_ms);
        self.last_temp_c = Some(current_temp_c);
    }

    fn finalize(&self, max_temp_c: f64) -> ThermalStageAnalysis {
        let first_hold_temp_c = self.first_hold_temp_c;
        let hold_mean_error_c = (!self.hold_error_c.is_empty())
            .then_some(self.hold_error_c.iter().sum::<f64>() / self.hold_error_c.len() as f64);
        let hold_max_above_target_c = self
            .hold_error_c
            .iter()
            .copied()
            .filter(|error_c| *error_c < 0.0)
            .map(f64::abs)
            .fold(0.0, f64::max);
        let hold_max_below_target_c = self
            .hold_error_c
            .iter()
            .copied()
            .filter(|error_c| *error_c > 0.0)
            .fold(0.0, f64::max);
        ThermalStageAnalysis {
            first_hold_temp_c,
            first_hold_error_c: first_hold_temp_c.map(|temp_c| self.target_temp_c - temp_c),
            residual_heat_after_hold_entry_c: first_hold_temp_c
                .map(|temp_c| (max_temp_c - temp_c).max(0.0)),
            approach_median_output_permille: percentile_u16(&self.approach_output_permille, 0.5),
            approach_median_slope_c_per_s: percentile_f64(&self.approach_slope_c_per_s, 0.5),
            hold_median_output_permille: percentile_u16(&self.hold_output_permille, 0.5),
            hold_p90_output_permille: percentile_u16(&self.hold_output_permille, 0.9),
            hold_mean_error_c,
            hold_max_above_target_c: (!self.hold_error_c.is_empty())
                .then_some(hold_max_above_target_c),
            hold_max_below_target_c: (!self.hold_error_c.is_empty())
                .then_some(hold_max_below_target_c),
            approach_sample_count: self.approach_output_permille.len(),
            hold_sample_count: self.hold_output_permille.len(),
            ..ThermalStageAnalysis::default()
        }
    }
}

impl ThermalApproachGuardTracker {
    fn new(target_temp_c: i16, hold_entry_centi_c: u16) -> Self {
        Self {
            hold_threshold_temp_c: f64::from(target_temp_c)
                - (f64::from(hold_entry_centi_c.max(1)) / 100.0),
            approach_started_at_ms: None,
            hold_threshold_crossed_at_ms: None,
            first_hold_at_ms: None,
            warmup_reentered_at_ms: None,
        }
    }

    fn observe(
        &mut self,
        current_temp_c: f64,
        elapsed_ms: u64,
        control_phase: Option<&str>,
    ) -> Option<&'static str> {
        match control_phase {
            Some("approach") => {
                let approach_started_at_ms = *self.approach_started_at_ms.get_or_insert(elapsed_ms);
                if current_temp_c >= self.hold_threshold_temp_c {
                    self.hold_threshold_crossed_at_ms.get_or_insert(elapsed_ms);
                }
                let approach_elapsed_ms = elapsed_ms.saturating_sub(approach_started_at_ms);
                if self.hold_threshold_crossed_at_ms.is_none() && approach_elapsed_ms > 10_000 {
                    return Some("approach_threshold_timeout");
                }
                if self.first_hold_at_ms.is_none() && approach_elapsed_ms > 30_000 {
                    return Some("approach_hold_timeout");
                }
            }
            Some("hold") => {
                if self.approach_started_at_ms.is_some() {
                    if current_temp_c >= self.hold_threshold_temp_c {
                        self.hold_threshold_crossed_at_ms.get_or_insert(elapsed_ms);
                    }
                    self.first_hold_at_ms.get_or_insert(elapsed_ms);
                }
            }
            Some("warmup") => {
                if self.approach_started_at_ms.is_some() && self.first_hold_at_ms.is_none() {
                    self.warmup_reentered_at_ms.get_or_insert(elapsed_ms);
                    return Some("approach_reentered_warmup");
                }
            }
            _ => {}
        }
        None
    }

    fn finalize(&self) -> ThermalApproachGuardAnalysis {
        ThermalApproachGuardAnalysis {
            hold_threshold_temp_c: self.hold_threshold_temp_c,
            approach_started_at_ms: self.approach_started_at_ms,
            hold_threshold_crossed_at_ms: self.hold_threshold_crossed_at_ms,
            first_hold_at_ms: self.first_hold_at_ms,
            warmup_reentered_at_ms: self.warmup_reentered_at_ms,
        }
    }
}

impl ThermalSourceWindowAnalysis {
    fn observe(&mut self, sample: &ThermalReplayStageSample) {
        self.sample_count = self.sample_count.saturating_add(1);
        if let Some(voltage_mv) = sample.source_voltage_mv {
            observe_series(&mut self.voltage_mv, voltage_mv as f64);
        }
        if let Some(current_ma) = sample.source_current_ma {
            observe_series(&mut self.current_ma, current_ma as f64);
        }
        if let Some(power_mw) = sample.source_power_mw {
            observe_series(&mut self.power_mw, power_mw as f64);
        }
    }

    fn to_value(&self) -> Option<Value> {
        if self.sample_count == 0 {
            return None;
        }
        let mut object = serde_json::Map::new();
        object.insert("sampleCount".into(), json!(self.sample_count));
        if let Some(voltage_mv) = self.voltage_mv.as_ref() {
            object.insert("voltageMv".into(), voltage_mv.to_value());
        }
        if let Some(current_ma) = self.current_ma.as_ref() {
            object.insert("currentMa".into(), current_ma.to_value());
        }
        if let Some(power_mw) = self.power_mw.as_ref() {
            object.insert("powerMw".into(), power_mw.to_value());
        }
        Some(Value::Object(object))
    }
}

impl ThermalStageSourceAnalysisBuilder {
    fn new(target_temp_c: i16) -> Self {
        Self {
            target_temp_c: f64::from(target_temp_c),
            first_hold_temp_c: None,
            approach: ThermalSourceWindowAnalysis::default(),
            hold: ThermalSourceWindowAnalysis::default(),
        }
    }

    fn observe(&mut self, sample: &ThermalReplayStageSample) {
        let error_c = self.target_temp_c - sample.current_temp_c;
        if sample.control_phase_in_hold {
            self.first_hold_temp_c.get_or_insert(sample.current_temp_c);
        }
        let hold_window_started = self.first_hold_temp_c.is_some();
        let near_target_approach_window = !hold_window_started
            && !sample.control_phase_in_hold
            && error_c >= 0.0
            && error_c <= 8.0
            && sample.current_temp_c <= self.target_temp_c + 0.5;

        if near_target_approach_window {
            self.approach.observe(sample);
        }
        if hold_window_started {
            self.hold.observe(sample);
        }
    }

    fn to_value(&self) -> Value {
        let mut object = serde_json::Map::new();
        if let Some(approach) = self.approach.to_value() {
            object.insert("approachSource".into(), approach);
        }
        if let Some(hold) = self.hold.to_value() {
            object.insert("holdSource".into(), hold);
        }
        Value::Object(object)
    }
}

impl ThermalFullSpeedStableTracker {
    const STABLE_BAND_C: f64 = 1.5;
    const STABLE_WINDOW_MS: u64 = 10_000;
    const LOW_TEMP_SETTLE_LIMIT_MS: u64 = 10_000;
    const HIGH_TEMP_SETTLE_LIMIT_MS: u64 = 5_000;
    const HIGH_TEMP_SETTLE_THRESHOLD_C: i16 = 150;

    fn settle_limit_ms_for_target(target_temp_c: i16) -> u64 {
        if target_temp_c > Self::HIGH_TEMP_SETTLE_THRESHOLD_C {
            Self::HIGH_TEMP_SETTLE_LIMIT_MS
        } else {
            Self::LOW_TEMP_SETTLE_LIMIT_MS
        }
    }

    fn settle_limit_ms(&self) -> u64 {
        Self::settle_limit_ms_for_target(self.target_temp_c.round() as i16)
    }

    fn new(target_temp_c: i16) -> Self {
        Self {
            target_temp_c: f64::from(target_temp_c),
            warmup_exited_at_ms: None,
            stable_window_started_at_ms: None,
            stable_window_verified_at_ms: None,
            settle_time_ms: None,
            failure_reason: None,
        }
    }

    fn observe(
        &mut self,
        current_temp_c: f64,
        elapsed_ms: u64,
        control_phase: Option<&str>,
    ) -> ThermalFullSpeedStableObservation {
        if self.warmup_exited_at_ms.is_none() {
            if control_phase.is_some_and(|phase| phase != "warmup") {
                self.warmup_exited_at_ms = Some(elapsed_ms);
            } else {
                return ThermalFullSpeedStableObservation::Pending;
            }
        }

        let warmup_exited_at_ms = self.warmup_exited_at_ms.unwrap_or(elapsed_ms);
        if self.stable_window_verified_at_ms.is_some() {
            return ThermalFullSpeedStableObservation::Verified;
        }

        // Stability is a physical temperature requirement. The controller may briefly use
        // Approach to recover heat loss while the plate remains inside the stable band; tying
        // this window to the phase label turns successful recovery into a false timeout.
        let inside_stable_window =
            (current_temp_c - self.target_temp_c).abs() <= Self::STABLE_BAND_C;
        if inside_stable_window {
            let stable_window_started_at_ms =
                *self.stable_window_started_at_ms.get_or_insert(elapsed_ms);
            let settle_time_ms = stable_window_started_at_ms.saturating_sub(warmup_exited_at_ms);
            self.settle_time_ms.get_or_insert(settle_time_ms);
            if settle_time_ms > self.settle_limit_ms() {
                self.failure_reason = Some("full_speed_to_stable_timeout");
                return ThermalFullSpeedStableObservation::Failed("full_speed_to_stable_timeout");
            }
            if elapsed_ms.saturating_sub(stable_window_started_at_ms) >= Self::STABLE_WINDOW_MS {
                self.stable_window_verified_at_ms = Some(elapsed_ms);
                return ThermalFullSpeedStableObservation::Verified;
            }
        } else if self.stable_window_started_at_ms.take().is_some() {
            self.settle_time_ms = None;
        }

        let latest_allowed_window_start_ms =
            warmup_exited_at_ms.saturating_add(self.settle_limit_ms());
        if self.stable_window_started_at_ms.is_none() && elapsed_ms > latest_allowed_window_start_ms
        {
            self.failure_reason = Some("full_speed_to_stable_timeout");
            return ThermalFullSpeedStableObservation::Failed("full_speed_to_stable_timeout");
        }
        ThermalFullSpeedStableObservation::Pending
    }

    fn finalize(&self) -> ThermalFullSpeedStableAnalysis {
        ThermalFullSpeedStableAnalysis {
            warmup_exited_at_ms: self.warmup_exited_at_ms,
            stable_window_started_at_ms: self.stable_window_started_at_ms,
            stable_window_verified_at_ms: self.stable_window_verified_at_ms,
            settle_time_ms: self.settle_time_ms,
            failure_reason: self.failure_reason,
        }
    }
}

impl ThermalHoldTracker {
    fn new(target_temp_c: i16, hold_duration: Duration) -> Self {
        Self {
            target_temp_c,
            hold_duration,
            entered_threshold_c: 0.5,
            stable_band_c: 3.0,
            first_hold_started_at: None,
            rise_time_ms: None,
            min_c: f64::INFINITY,
            max_c: f64::NEG_INFINITY,
        }
    }

    fn observe(
        &mut self,
        current_temp_c: f64,
        elapsed_ms: u64,
        now: tokio::time::Instant,
        control_phase_in_hold: bool,
    ) -> ThermalHoldObservation {
        let target_temp_c = f64::from(self.target_temp_c);
        if self.first_hold_started_at.is_none() {
            if !control_phase_in_hold {
                return ThermalHoldObservation::Warmup;
            }
            if current_temp_c < target_temp_c - self.entered_threshold_c {
                return ThermalHoldObservation::Warmup;
            }
            if (current_temp_c - target_temp_c).abs() > self.stable_band_c {
                return ThermalHoldObservation::Warmup;
            }
        }

        let first_hold_started_at = *self.first_hold_started_at.get_or_insert_with(|| {
            self.rise_time_ms.get_or_insert(elapsed_ms);
            now
        });
        self.min_c = self.min_c.min(current_temp_c);
        self.max_c = self.max_c.max(current_temp_c);
        if now.saturating_duration_since(first_hold_started_at) >= self.hold_duration {
            ThermalHoldObservation::Completed
        } else {
            ThermalHoldObservation::Hold
        }
    }

    fn rise_time_ms(&self) -> Option<u64> {
        self.rise_time_ms
    }

    fn peak_to_peak_c(&self) -> f64 {
        if self.min_c.is_finite() && self.max_c.is_finite() {
            self.max_c - self.min_c
        } else {
            f64::INFINITY
        }
    }
}

fn thermal_runtime_drop_reason(
    status: &Value,
    target_temp_c: i16,
    last_uptime_seconds: Option<u64>,
) -> Option<ThermalRuntimeDropReason> {
    let uptime_seconds = status.get("uptimeSeconds").and_then(Value::as_u64);
    if let Some((last, current)) = last_uptime_seconds.zip(uptime_seconds)
        && current < last
    {
        return Some(ThermalRuntimeDropReason::UptimeReset);
    }
    if status
        .get("heaterFaultReason")
        .is_some_and(|reason| !reason.is_null())
    {
        return Some(ThermalRuntimeDropReason::LatchedFault);
    }
    if status
        .get("targetTempC")
        .and_then(Value::as_i64)
        .is_some_and(|target| target != i64::from(target_temp_c))
    {
        return Some(ThermalRuntimeDropReason::WrongTarget);
    }
    if status
        .get("mode")
        .and_then(Value::as_str)
        .is_some_and(|mode| mode != "sampling")
    {
        return Some(ThermalRuntimeDropReason::WrongMode);
    }
    if status
        .get("heaterEnabled")
        .and_then(Value::as_bool)
        .is_some_and(|heater_enabled| !heater_enabled)
    {
        return Some(ThermalRuntimeDropReason::HeaterDisarmed);
    }
    None
}

fn thermal_recoverable_sensor_fault(status: &Value) -> bool {
    matches!(
        status.get("heaterFaultReason").and_then(Value::as_str),
        Some("sensor-glitch" | "sensor-open" | "sensor-short" | "adc-read-failed")
    ) && status.get("mode").and_then(Value::as_str) != Some("fault")
}

impl ThermalStageResult {
    fn to_value(&self) -> Value {
        json!({
            "targetTempC": self.target_temp_c,
            "riseTimeMs": self.rise_time_ms,
            "maxOvershootC": self.max_overshoot_c,
            "holdPeakToPeakC": self.hold_peak_to_peak_c,
            "sampleCount": self.sample_count,
            "stopReason": self.stop_reason,
            "terminalRuntimeDropReason": self.terminal_runtime_drop_reason,
            "guard": {
                "holdThresholdTempC": self.guard.hold_threshold_temp_c,
                "approachStartedAtMs": self.guard.approach_started_at_ms,
                "holdThresholdCrossedAtMs": self.guard.hold_threshold_crossed_at_ms,
                "firstHoldAtMs": self.guard.first_hold_at_ms,
                "warmupReenteredAtMs": self.guard.warmup_reentered_at_ms,
            },
            "fullSpeedToStable": {
                "limitMs": ThermalFullSpeedStableTracker::settle_limit_ms_for_target(self.target_temp_c),
                "stableBandC": ThermalFullSpeedStableTracker::STABLE_BAND_C,
                "stableWindowMs": ThermalFullSpeedStableTracker::STABLE_WINDOW_MS,
                "warmupExitedAtMs": self.full_speed_to_stable.warmup_exited_at_ms,
                "stableWindowStartedAtMs": self.full_speed_to_stable.stable_window_started_at_ms,
                "stableWindowVerifiedAtMs": self.full_speed_to_stable.stable_window_verified_at_ms,
                "settleTimeMs": self.full_speed_to_stable.settle_time_ms,
                "failureReason": self.full_speed_to_stable.failure_reason,
            },
            "analysis": {
                "firstHoldTempC": self.analysis.first_hold_temp_c,
                "firstHoldErrorC": self.analysis.first_hold_error_c,
                "residualHeatAfterHoldEntryC": self.analysis.residual_heat_after_hold_entry_c,
                "approachMedianOutputPermille": self.analysis.approach_median_output_permille,
                "approachMedianSlopeCPerS": self.analysis.approach_median_slope_c_per_s,
                "holdMedianOutputPermille": self.analysis.hold_median_output_permille,
                "holdP90OutputPermille": self.analysis.hold_p90_output_permille,
                "holdMeanErrorC": self.analysis.hold_mean_error_c,
                "holdMaxAboveTargetC": self.analysis.hold_max_above_target_c,
                "holdMaxBelowTargetC": self.analysis.hold_max_below_target_c,
                "approachCurveFitBasis": self.analysis.approach_curve_fit_basis,
                "approachCurveStartTempC": self.analysis.approach_curve_start_temp_c,
                "approachCurveFittedMs": self.analysis.approach_curve_fitted_ms,
                "approachCurvePreferredMs": self.analysis.approach_curve_preferred_ms,
                "approachCurveLimitMs": self.analysis.approach_curve_limit_ms,
                "approachCurveMaxAboveC": self.analysis.approach_curve_max_above_c,
                "approachCurveMaxBelowC": self.analysis.approach_curve_max_below_c,
                "approachCurveMeanAbsErrorC": self.analysis.approach_curve_mean_abs_error_c,
                "approachCurveOscillationC": self.analysis.approach_curve_oscillation_c,
                "approachCurveDeviationClass": self.analysis.approach_curve_deviation_class,
                "approachCurveTailUsesHalfFloor": self.analysis.approach_curve_tail_uses_half_floor,
                "approachSampleCount": self.analysis.approach_sample_count,
                "holdSampleCount": self.analysis.hold_sample_count,
            },
        })
    }
}

fn thermal_profile_package_from_value(imported: Value) -> Value {
    imported
        .get("profile")
        .cloned()
        .or_else(|| {
            imported
                .get("thermalControlProfile")
                .and_then(|thermal_control_profile| thermal_control_profile.get("profile"))
                .cloned()
        })
        .unwrap_or(imported)
}

fn parse_thermal_targets_from_summary(
    summary: &Value,
    key: &str,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    let values = summary
        .get("parameters")
        .and_then(|parameters| parameters.get(key))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("thermal summary missing parameters.{key}"),
            )
        })?;
    let mut targets = Vec::with_capacity(values.len());
    for value in values {
        let target = value.as_i64().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("thermal summary parameters.{key} contains a non-integer target"),
            )
        })?;
        targets.push(i16::try_from(target).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("thermal summary parameters.{key} target out of range"),
            )
        })?);
    }
    Ok(targets)
}

fn thermal_self_test_evaluation_mode_from_summary(
    summary: &Value,
) -> ThermalSelfTestEvaluationMode {
    match summary
        .get("parameters")
        .and_then(|parameters| parameters.get("evaluationMode"))
        .and_then(Value::as_str)
    {
        Some("tuning-scout") => ThermalSelfTestEvaluationMode::TuningScout,
        _ => ThermalSelfTestEvaluationMode::HoldConfirm,
    }
}

fn require_value_u64(
    value: &Value,
    key: &str,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    value.get(key).and_then(Value::as_u64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("value missing integer field: {key}"),
        )
        .into()
    })
}

fn require_value_i16(
    value: &Value,
    key: &str,
) -> Result<i16, Box<dyn std::error::Error + Send + Sync>> {
    let parsed = value.get(key).and_then(Value::as_i64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("value missing integer field: {key}"),
        )
    })?;
    i16::try_from(parsed).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("value field out of range: {key}"),
        )
        .into()
    })
}

fn require_value_f64(
    value: &Value,
    key: &str,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    value.get(key).and_then(Value::as_f64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("value missing numeric field: {key}"),
        )
        .into()
    })
}

fn require_value_str<'a>(
    value: &'a Value,
    key: &str,
) -> Result<&'a str, Box<dyn std::error::Error + Send + Sync>> {
    value.get(key).and_then(Value::as_str).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("value missing string field: {key}"),
        )
        .into()
    })
}

fn thermal_stage_result_from_value(
    value: &Value,
) -> Result<ThermalStageResult, Box<dyn std::error::Error + Send + Sync>> {
    let stop_reason = match require_value_str(value, "stopReason")? {
        "completed" => "completed",
        "timeout" => "timeout",
        "runtime_lost" => "runtime_lost",
        "uptime_reset" => "uptime_reset",
        "latched_fault" => "latched_fault",
        "heater_disarmed" => "heater_disarmed",
        "wrong_mode" => "wrong_mode",
        "wrong_target" => "wrong_target",
        "sample_rate_below_3hz" => "sample_rate_below_3hz",
        "status_request_failed" => "status_request_failed",
        "heater_no_output" => "heater_no_output",
        "warmup_timeout" => "warmup_timeout",
        "approach_threshold_timeout" => "approach_threshold_timeout",
        "approach_hold_timeout" => "approach_hold_timeout",
        "approach_reentered_warmup" => "approach_reentered_warmup",
        "full_speed_to_stable_timeout" => "full_speed_to_stable_timeout",
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported thermal stop reason in replay: {other}"),
            )
            .into());
        }
    };
    let hold_peak_to_peak_c = match value.get("holdPeakToPeakC").and_then(Value::as_f64) {
        Some(value) => value,
        None if stop_reason != "completed" => f64::INFINITY,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "completed thermal stage missing numeric field: holdPeakToPeakC",
            )
            .into());
        }
    };
    Ok(ThermalStageResult {
        target_temp_c: require_value_i16(value, "targetTempC")?,
        rise_time_ms: require_value_u64(value, "riseTimeMs")?,
        max_overshoot_c: require_value_f64(value, "maxOvershootC")?,
        hold_peak_to_peak_c,
        sample_count: require_value_u64(value, "sampleCount")? as usize,
        stop_reason,
        terminal_runtime_drop_reason: value
            .get("terminalRuntimeDropReason")
            .and_then(Value::as_str)
            .and_then(|reason| match reason {
                "uptime_reset" => Some("uptime_reset"),
                "latched_fault" => Some("latched_fault"),
                "heater_disarmed" => Some("heater_disarmed"),
                "wrong_mode" => Some("wrong_mode"),
                "wrong_target" => Some("wrong_target"),
                _ => None,
            }),
        analysis: ThermalStageAnalysis::default(),
        guard: ThermalApproachGuardAnalysis {
            hold_threshold_temp_c: value
                .get("guard")
                .and_then(|guard| guard.get("holdThresholdTempC"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            approach_started_at_ms: value
                .get("guard")
                .and_then(|guard| guard.get("approachStartedAtMs"))
                .and_then(Value::as_u64),
            hold_threshold_crossed_at_ms: value
                .get("guard")
                .and_then(|guard| guard.get("holdThresholdCrossedAtMs"))
                .and_then(Value::as_u64),
            first_hold_at_ms: value
                .get("guard")
                .and_then(|guard| guard.get("firstHoldAtMs"))
                .and_then(Value::as_u64),
            warmup_reentered_at_ms: value
                .get("guard")
                .and_then(|guard| guard.get("warmupReenteredAtMs"))
                .and_then(Value::as_u64),
        },
        full_speed_to_stable: ThermalFullSpeedStableAnalysis {
            warmup_exited_at_ms: value
                .get("fullSpeedToStable")
                .and_then(|value| value.get("warmupExitedAtMs"))
                .and_then(Value::as_u64),
            stable_window_started_at_ms: value
                .get("fullSpeedToStable")
                .and_then(|value| value.get("stableWindowStartedAtMs"))
                .and_then(Value::as_u64),
            stable_window_verified_at_ms: value
                .get("fullSpeedToStable")
                .and_then(|value| value.get("stableWindowVerifiedAtMs"))
                .and_then(Value::as_u64),
            settle_time_ms: value
                .get("fullSpeedToStable")
                .and_then(|value| value.get("settleTimeMs"))
                .and_then(Value::as_u64),
            failure_reason: value
                .get("fullSpeedToStable")
                .and_then(|value| value.get("failureReason"))
                .and_then(Value::as_str)
                .and_then(|reason| match reason {
                    "full_speed_to_stable_timeout" => Some("full_speed_to_stable_timeout"),
                    _ => None,
                }),
        },
    })
}

fn read_ndjson_values(path: &Path) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let reader = BufReader::new(File::open(path)?);
    let mut values = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str(&line)?);
    }
    Ok(values)
}

fn thermal_replay_stage_samples(
    samples: &[Value],
    target_temp_c: i16,
) -> Result<Vec<ThermalReplayStageSample>, Box<dyn std::error::Error + Send + Sync>> {
    let mut stage_samples = Vec::new();
    for sample in samples {
        if sample.get("testPhase").and_then(Value::as_str) != Some("applied") {
            continue;
        }
        if sample.get("targetTempC").and_then(Value::as_i64) != Some(i64::from(target_temp_c)) {
            continue;
        }
        if sample.get("phase").and_then(Value::as_str) == Some("runtime_rearm") {
            continue;
        }
        let heater = sample.get("heaterTelemetry").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "thermal replay sample missing heaterTelemetry",
            )
        })?;
        let status = sample.get("status").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "thermal replay sample missing status",
            )
        })?;
        let elapsed_ms = require_value_u64(sample, "elapsedMs")?;
        let current_temp_c = require_status_f64(heater, "currentTempC")?;
        let heater_output_percent =
            require_status_u64(heater, "heaterOutputPercent")?.min(u64::from(u8::MAX)) as u8;
        let control_phase = status
            .get("heaterControlPhase")
            .or_else(|| sample.get("phase"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let control_phase_in_hold = control_phase.as_deref() == Some("hold");
        let source_telemetry = sample.get("sourceTelemetry");
        stage_samples.push(ThermalReplayStageSample {
            elapsed_ms,
            current_temp_c,
            heater_output_percent,
            control_phase,
            control_phase_in_hold,
            source_voltage_mv: source_telemetry
                .and_then(|value| value.get("voltageMv"))
                .and_then(Value::as_u64)
                .or_else(|| sample.get("sourceActualVoltageMv").and_then(Value::as_u64)),
            source_current_ma: source_telemetry
                .and_then(|value| value.get("currentMa"))
                .and_then(Value::as_u64)
                .or_else(|| sample.get("sourceActualCurrentMa").and_then(Value::as_u64)),
            source_power_mw: source_telemetry
                .and_then(|value| value.get("powerMw"))
                .and_then(Value::as_u64)
                .or_else(|| sample.get("sourceActualPowerMw").and_then(Value::as_u64)),
        });
    }
    stage_samples.sort_by_key(|sample| sample.elapsed_ms);
    Ok(stage_samples)
}

fn thermal_replay_stage_analysis(
    samples: &[ThermalReplayStageSample],
    target_temp_c: i16,
) -> ThermalStageAnalysis {
    let mut analyzer = ThermalStageAnalyzer::new(target_temp_c);
    let mut max_temp_c = f64::NEG_INFINITY;
    for sample in samples {
        max_temp_c = max_temp_c.max(sample.current_temp_c);
        analyzer.observe(
            sample.current_temp_c,
            sample.heater_output_percent,
            sample.elapsed_ms,
            sample.control_phase_in_hold,
        );
    }
    if max_temp_c.is_finite() {
        let mut analysis = analyzer.finalize(max_temp_c);
        thermal_stage_populate_approach_curve_analysis(&mut analysis, samples, target_temp_c);
        analysis
    } else {
        ThermalStageAnalysis::default()
    }
}

fn thermal_approach_curve_reference_temp_c(
    start_temp_c: f64,
    target_temp_c: f64,
    elapsed_from_start_ms: u64,
    fitted_ms: u64,
) -> f64 {
    let normalized = if fitted_ms == 0 {
        1.0
    } else {
        (elapsed_from_start_ms as f64 / fitted_ms as f64).clamp(0.0, 1.0)
    };
    let progress = 1.0 - (1.0 - normalized).powi(2);
    start_temp_c + (target_temp_c - start_temp_c) * progress
}

fn thermal_stage_approach_curve_sample_series(
    samples: &[ThermalReplayStageSample],
    target_temp_c: i16,
    guard: &ThermalApproachGuardAnalysis,
    analysis: &ThermalStageAnalysis,
) -> Vec<Option<f64>> {
    let Some(start_temp_c) = analysis.approach_curve_start_temp_c else {
        return vec![None; samples.len()];
    };
    let Some(fitted_ms) = analysis.approach_curve_fitted_ms else {
        return vec![None; samples.len()];
    };
    let Some(started_at_ms) = guard
        .approach_started_at_ms
        .or(guard.hold_threshold_crossed_at_ms)
        .or(guard.first_hold_at_ms)
        .or_else(|| samples.first().map(|sample| sample.elapsed_ms))
    else {
        return vec![None; samples.len()];
    };
    let stop_at_ms = guard
        .first_hold_at_ms
        .or(guard.hold_threshold_crossed_at_ms)
        .unwrap_or_else(|| started_at_ms.saturating_add(fitted_ms));
    let target_temp_c = f64::from(target_temp_c);

    samples
        .iter()
        .map(|sample| {
            if sample.elapsed_ms < started_at_ms || sample.elapsed_ms > stop_at_ms {
                return None;
            }
            Some(thermal_approach_curve_reference_temp_c(
                start_temp_c,
                target_temp_c,
                sample.elapsed_ms.saturating_sub(started_at_ms),
                fitted_ms,
            ))
        })
        .collect()
}

fn thermal_stage_populate_approach_curve_analysis(
    analysis: &mut ThermalStageAnalysis,
    samples: &[ThermalReplayStageSample],
    target_temp_c: i16,
) {
    let Some(start_index) = samples
        .iter()
        .position(|sample| sample.control_phase.as_deref() != Some("warmup"))
    else {
        return;
    };
    let Some(start_sample) = samples.get(start_index) else {
        return;
    };
    let end_index = samples
        .iter()
        .position(|sample| sample.control_phase_in_hold)
        .unwrap_or_else(|| samples.len().saturating_sub(1));
    let Some(end_sample) = samples.get(end_index) else {
        return;
    };

    analysis.approach_curve_fit_basis = Some("target_error_from_approach_start");
    analysis.approach_curve_start_temp_c = Some(start_sample.current_temp_c);
    analysis.approach_curve_preferred_ms = Some(THERMAL_APPROACH_CURVE_PREFERRED_MS);
    analysis.approach_curve_limit_ms = Some(THERMAL_APPROACH_CURVE_LIMIT_MS);
    analysis.approach_curve_tail_uses_half_floor = Some(true);

    let raw_duration_ms = end_sample
        .elapsed_ms
        .saturating_sub(start_sample.elapsed_ms);
    let fitted_ms = raw_duration_ms.clamp(
        THERMAL_APPROACH_CURVE_PREFERRED_MS,
        THERMAL_APPROACH_CURVE_LIMIT_MS,
    );
    analysis.approach_curve_fitted_ms = Some(fitted_ms);

    let target_temp_c = f64::from(target_temp_c);
    let target_delta_c = target_temp_c - start_sample.current_temp_c;
    if raw_duration_ms == 0 || target_delta_c <= 0.0 {
        analysis.approach_curve_deviation_class = Some("insufficient_evidence");
        return;
    }

    let mut max_above_c = 0.0f64;
    let mut max_below_c = 0.0f64;
    let mut mean_abs_error_sum_c = 0.0f64;
    let mut deviation_count = 0usize;
    let mut last_sign = 0i8;
    let mut sign_changes = 0usize;

    for sample in samples
        .iter()
        .take(end_index.saturating_add(1))
        .skip(start_index)
    {
        let elapsed_from_start_ms = sample.elapsed_ms.saturating_sub(start_sample.elapsed_ms);
        let reference_temp_c = thermal_approach_curve_reference_temp_c(
            start_sample.current_temp_c,
            target_temp_c,
            elapsed_from_start_ms,
            fitted_ms,
        );
        let deviation_c = sample.current_temp_c - reference_temp_c;
        max_above_c = max_above_c.max(deviation_c.max(0.0));
        max_below_c = max_below_c.max((-deviation_c).max(0.0));
        mean_abs_error_sum_c += deviation_c.abs();
        deviation_count = deviation_count.saturating_add(1);

        let sign = if deviation_c > THERMAL_APPROACH_CURVE_SIGNIFICANT_DEVIATION_C {
            1
        } else if deviation_c < -THERMAL_APPROACH_CURVE_SIGNIFICANT_DEVIATION_C {
            -1
        } else {
            0
        };
        if sign != 0 {
            if last_sign != 0 && sign != last_sign {
                sign_changes = sign_changes.saturating_add(1);
            }
            last_sign = sign;
        }
    }

    analysis.approach_curve_max_above_c = Some(max_above_c);
    analysis.approach_curve_max_below_c = Some(max_below_c);
    if deviation_count > 0 {
        analysis.approach_curve_mean_abs_error_c =
            Some(mean_abs_error_sum_c / deviation_count as f64);
    }
    let oscillation_c = max_above_c.min(max_below_c);
    analysis.approach_curve_oscillation_c = Some(oscillation_c);

    analysis.approach_curve_deviation_class = Some(
        if max_above_c >= 1.0 && max_above_c > max_below_c + THERMAL_APPROACH_CURVE_CLASS_MARGIN_C {
            "brake_late_or_residual"
        } else if max_below_c >= 1.0
            && max_below_c > max_above_c + THERMAL_APPROACH_CURVE_CLASS_MARGIN_C
        {
            "underpowered_or_early_coast"
        } else if sign_changes >= 2
            && max_above_c >= THERMAL_APPROACH_CURVE_SIGNIFICANT_DEVIATION_C
            && max_below_c >= THERMAL_APPROACH_CURVE_SIGNIFICANT_DEVIATION_C
        {
            "oscillatory_near_target"
        } else {
            "on_curve"
        },
    );
}

fn thermal_replay_stage_source_analysis(
    samples: &[ThermalReplayStageSample],
    target_temp_c: i16,
) -> Value {
    let mut builder = ThermalStageSourceAnalysisBuilder::new(target_temp_c);
    for sample in samples {
        builder.observe(sample);
    }
    builder.to_value()
}

fn thermal_stage_value_attach_source_analysis(
    stage_value: &mut Value,
    stage_samples: &[ThermalReplayStageSample],
    target_temp_c: i16,
) {
    let Some(analysis) = stage_value
        .get_mut("analysis")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let source_analysis = thermal_replay_stage_source_analysis(stage_samples, target_temp_c);
    let Some(source_analysis) = source_analysis.as_object() else {
        return;
    };
    for (key, value) in source_analysis {
        analysis.insert(key.clone(), value.clone());
    }
}

fn thermal_summary_attach_replay_source_analysis(
    summary: &mut Value,
    samples: &[Value],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(applied) = summary.get_mut("applied").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for stage_value in applied.iter_mut() {
        let target_temp_c = stage_value
            .get("targetTempC")
            .and_then(Value::as_i64)
            .and_then(|value| i16::try_from(value).ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "thermal summary applied result missing targetTempC",
                )
            })?;
        let stage_samples = thermal_replay_stage_samples(samples, target_temp_c)?;
        thermal_stage_value_attach_source_analysis(stage_value, &stage_samples, target_temp_c);
    }
    Ok(())
}

fn thermal_summary_attach_source_analysis_from_ndjson(
    summary: &mut Value,
    samples_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let samples = read_ndjson_values(samples_path)?;
    thermal_summary_attach_replay_source_analysis(summary, &samples)
}

fn thermal_replay_full_speed_to_stable(
    samples: &[ThermalReplayStageSample],
    target_temp_c: i16,
) -> ThermalFullSpeedStableAnalysis {
    let mut tracker = ThermalFullSpeedStableTracker::new(target_temp_c);
    for sample in samples {
        let _ = tracker.observe(
            sample.current_temp_c,
            sample.elapsed_ms,
            sample.control_phase.as_deref(),
        );
    }
    tracker.finalize()
}

fn thermal_candidate_point_from_heater_parameters(
    heater_parameters: &Value,
) -> Result<ThermalCandidatePoint, Box<dyn std::error::Error + Send + Sync>> {
    let target_temp_c = require_value_i16(heater_parameters, "targetTempC")?;
    let default_point = thermal_default_target_point(target_temp_c);
    Ok(ThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c: heater_parameters
            .get("brakeDistanceCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.brake_distance_centi_c),
        warmup_power_permille: 1_000,
        approach_power_permille: heater_parameters
            .get("approachPowerPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.approach_power_permille),
        approach_floor_power_permille: heater_parameters
            .get("approachFloorPowerPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.approach_floor_power_permille),
        approach_damping_exponent_permille: heater_parameters
            .get("approachDampingExponentPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.approach_damping_exponent_permille),
        approach_tail_window_centi_c: heater_parameters
            .get("approachTailWindowCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.approach_tail_window_centi_c),
        hold_power_permille: heater_parameters
            .get("holdPowerPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_power_permille),
        hold_reheat_power_permille: heater_parameters
            .get("holdReheatPowerPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_reheat_power_permille),
        hold_entry_centi_c: heater_parameters
            .get("holdEntryCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_entry_centi_c),
        hold_exit_centi_c: heater_parameters
            .get("holdExitCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_exit_centi_c),
        hold_on_centi_c: heater_parameters
            .get("holdOnCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_on_centi_c),
        hold_off_centi_c: heater_parameters
            .get("holdOffCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_off_centi_c),
        overshoot_cutoff_centi_c: heater_parameters
            .get("overshootCutoffCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.overshoot_cutoff_centi_c),
        hold_kp_permille_per_c: heater_parameters
            .get("holdKpPermillePerC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_kp_permille_per_c),
        hold_ki_permille_per_c_tick: heater_parameters
            .get("holdKiPermillePerCTick")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_ki_permille_per_c_tick),
        hold_blend_ticks: heater_parameters
            .get("holdBlendTicks")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_blend_ticks),
        approach_lead_ticks: heater_parameters
            .get("approachLeadTicks")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.approach_lead_ticks),
        hold_lead_ticks: heater_parameters
            .get("holdLeadTicks")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_lead_ticks),
    })
}

fn thermal_replay_applied_profile(
    summary: &Value,
    samples: &[Value],
    target_temps_c: &[i16],
) -> Result<ThermalCandidateProfile, Box<dyn std::error::Error + Send + Sync>> {
    let fallback_bank = summary
        .get("source")
        .and_then(|source| source.get("resolvedBank"))
        .and_then(Value::as_str)
        .unwrap_or("pps3a");
    let seed_profile_path = summary
        .get("parameters")
        .and_then(|parameters| parameters.get("seedProfileFile"))
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let mut profile = if let Some(candidate_profile) = summary.get("candidateProfile") {
        thermal_candidate_profile_from_value(candidate_profile.clone())
    } else if let Some(seed_profile_path) = seed_profile_path
        && seed_profile_path.exists()
    {
        thermal_candidate_profile_from_value(serde_json::from_slice(&fs::read(seed_profile_path)?)?)
    } else {
        load_thermal_default_seed_candidate_profile(fallback_bank)?.0
    };
    if let Some(settings_value) = samples.iter().find_map(|sample| {
        sample
            .get("testPhase")
            .and_then(Value::as_str)
            .filter(|phase| *phase == "applied")
            .and_then(|_| sample.get("heaterParameters"))
            .and_then(|heater_parameters| heater_parameters.get("settings"))
            .cloned()
    }) {
        profile.settings = thermal_candidate_profile_from_value(json!({
            "settings": settings_value,
            "points": []
        }))
        .settings;
    }
    for &target_temp_c in target_temps_c {
        let Some(heater_parameters) = samples.iter().find_map(|sample| {
            (sample.get("testPhase").and_then(Value::as_str) == Some("applied")
                && sample.get("targetTempC").and_then(Value::as_i64)
                    == Some(i64::from(target_temp_c)))
            .then(|| sample.get("heaterParameters").cloned())
            .flatten()
        }) else {
            continue;
        };
        if let Some(point) = thermal_candidate_point_mut(&mut profile, target_temp_c) {
            *point = thermal_candidate_point_from_heater_parameters(&heater_parameters)?;
        }
    }
    Ok(profile)
}

async fn retune_thermal_self_test_run(
    client: &Client,
    default_devd: &str,
    args: ThermalRetuneArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let apply_preview = args.apply_preview;
    let target = args.target.clone();
    let summary_path = args.run_dir.join("run.json");
    let samples_path = args.run_dir.join("samples.ndjson");
    let summary: Value = serde_json::from_slice(&fs::read(&summary_path)?)?;
    if summary.get("kind").and_then(Value::as_str) != Some("thermal_self_test") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thermal retune requires a thermal self-test run.json",
        )
        .into());
    }
    let target_temps_c = parse_thermal_targets_from_summary(&summary, "targetsC")?;
    let optimize_targets_c = if let Some(optimize_targets_c) = args.optimize_targets_c.as_deref() {
        parse_thermal_targets(Some(optimize_targets_c))?
    } else {
        parse_thermal_targets_from_summary(&summary, "optimizeTargetsC")?
    };
    let original_applied = summary
        .get("applied")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "thermal summary missing applied results",
            )
        })?;
    let samples = read_ndjson_values(&samples_path)?;
    let mut candidate_profile =
        thermal_replay_applied_profile(&summary, &samples, &target_temps_c)?;
    let mut candidate_profile_value = thermal_candidate_profile_to_value(&candidate_profile);
    let mut applied_results = Vec::new();
    let mut tuning_steps = Vec::<Value>::new();

    for (stage_index, original_result) in original_applied.iter().enumerate() {
        let mut replayed = thermal_stage_result_from_value(original_result)?;
        let stage_samples = thermal_replay_stage_samples(&samples, replayed.target_temp_c)?;
        replayed.sample_count = stage_samples.len();
        replayed.analysis = thermal_replay_stage_analysis(&stage_samples, replayed.target_temp_c);
        replayed.full_speed_to_stable =
            thermal_replay_full_speed_to_stable(&stage_samples, replayed.target_temp_c);
        if optimize_targets_c.contains(&replayed.target_temp_c) {
            if let Some(point) =
                thermal_candidate_point_mut(&mut candidate_profile, replayed.target_temp_c)
            {
                *point = tune_thermal_candidate_point(*point, &replayed);
            }
            thermal_rebuild_profile_from_anchor_targets(
                &mut candidate_profile,
                &optimize_targets_c,
            );
            candidate_profile_value = thermal_candidate_profile_to_value(&candidate_profile);
            tuning_steps.push(json!({
                "stageIndex": stage_index,
                "targetTempC": replayed.target_temp_c,
                "result": replayed.to_value(),
                "candidateProfile": candidate_profile_value.clone(),
            }));
        }
        applied_results.push(replayed);
    }

    let evaluation_mode = thermal_self_test_evaluation_mode_from_summary(&summary);
    let validation =
        validate_thermal_applied_results(&applied_results, &target_temps_c, evaluation_mode);
    let replay_summary_path = args.run_dir.join("run.replayed.json");
    let replay_candidate_path = args.run_dir.join("thermal-profile.replayed.candidate.json");
    let mut replay_summary = json!({
        "kind": "thermal_self_test_replay",
        "ok": validation.get("passed").and_then(Value::as_bool) == Some(true),
        "runId": summary.get("runId").cloned().unwrap_or(Value::Null),
        "replayOf": summary_path,
        "target": summary.get("target").cloned().unwrap_or(Value::Null),
        "source": summary.get("source").cloned().unwrap_or(Value::Null),
        "parameters": {
            "targetsC": target_temps_c,
            "optimizeTargetsC": optimize_targets_c,
            "sampleIntervalMs": summary.pointer("/parameters/sampleIntervalMs").cloned().unwrap_or(Value::Null),
            "effectiveSampleIntervalMs": summary.pointer("/parameters/effectiveSampleIntervalMs").cloned().unwrap_or(Value::Null),
            "holdSeconds": summary.pointer("/parameters/holdSeconds").cloned().unwrap_or(Value::Null),
            "stageTimeoutSeconds": summary.pointer("/parameters/stageTimeoutSeconds").cloned().unwrap_or(Value::Null),
            "runtimeRearmAttempts": summary.pointer("/parameters/runtimeRearmAttempts").cloned().unwrap_or(Value::Null),
            "cooldownTempC": summary.pointer("/parameters/cooldownTempC").cloned().unwrap_or(Value::Null),
            "cooldownTimeoutSeconds": summary.pointer("/parameters/cooldownTimeoutSeconds").cloned().unwrap_or(Value::Null),
            "limits": summary.pointer("/parameters/limits").cloned().unwrap_or(Value::Null),
            "seedProfileFile": summary.pointer("/parameters/seedProfileFile").cloned().unwrap_or(Value::Null),
            "evaluationMode": evaluation_mode.as_str(),
        },
        "files": {
            "runDir": args.run_dir,
            "summaryPath": replay_summary_path,
            "samplesPath": samples_path,
            "candidateProfilePath": replay_candidate_path,
        },
        "sampleCount": summary.get("sampleCount").cloned().unwrap_or(Value::Null),
        "candidateProfile": candidate_profile_value.clone(),
        "profilePersistence": "not_saved",
        "tuningSteps": tuning_steps,
        "applied": applied_results.iter().map(ThermalStageResult::to_value).collect::<Vec<_>>(),
        "validation": validation,
    });
    thermal_summary_attach_replay_source_analysis(&mut replay_summary, &samples)?;
    fs::write(
        &replay_candidate_path,
        serde_json::to_vec_pretty(&candidate_profile_value)?,
    )?;
    fs::write(
        &replay_summary_path,
        serde_json::to_vec_pretty(&replay_summary)?,
    )?;
    if !apply_preview {
        return Ok(replay_summary);
    }

    let target_value = thermal_retune_requested_target_value(&target, default_devd);
    let resolved = match resolve_target(target.clone(), default_devd) {
        Ok(resolved) => resolved,
        Err(error) => {
            replay_summary["applyPreview"] = json!({
                "op": "thermalControlProfile.preview",
                "ok": false,
                "target": target_value,
                "previewResponse": null,
                "statusReadback": null,
                "error": error.to_string(),
            });
            fs::write(
                &replay_summary_path,
                serde_json::to_vec_pretty(&replay_summary)?,
            )?;
            return Err(error);
        }
    };
    let preview_response = match request_with_lease(
        client,
        resolved.clone(),
        Method::PUT,
        "/runtime",
        Some(json!({
            "thermalControlProfile": {
                "op": "preview",
                "profile": candidate_profile_value,
            }
        })),
    )
    .await
    {
        Ok(response) => response,
        Err(error) => {
            replay_summary["applyPreview"] = json!({
                "op": "thermalControlProfile.preview",
                "ok": false,
                "target": target_value,
                "previewResponse": null,
                "statusReadback": null,
                "error": error.to_string(),
            });
            fs::write(
                &replay_summary_path,
                serde_json::to_vec_pretty(&replay_summary)?,
            )?;
            return Err(error);
        }
    };
    let status = match request_with_lease(client, resolved, Method::GET, "/status", None).await {
        Ok(status) => status,
        Err(error) => {
            replay_summary["applyPreview"] = json!({
                "op": "thermalControlProfile.preview",
                "ok": false,
                "target": target_value,
                "previewResponse": thermal_retune_status_summary(&preview_response),
                "statusReadback": null,
                "error": error.to_string(),
            });
            fs::write(
                &replay_summary_path,
                serde_json::to_vec_pretty(&replay_summary)?,
            )?;
            return Err(error);
        }
    };
    if status
        .get("thermalControlProfilePreview")
        .and_then(Value::as_bool)
        != Some(true)
    {
        let error = io::Error::new(
            io::ErrorKind::InvalidData,
            "thermal retune preview apply did not enable thermalControlProfilePreview",
        );
        replay_summary["applyPreview"] = json!({
            "op": "thermalControlProfile.preview",
            "ok": false,
            "target": target_value,
            "previewResponse": thermal_retune_status_summary(&preview_response),
            "statusReadback": thermal_retune_status_summary(&status),
            "error": error.to_string(),
        });
        fs::write(
            &replay_summary_path,
            serde_json::to_vec_pretty(&replay_summary)?,
        )?;
        return Err(error.into());
    }
    replay_summary["applyPreview"] = json!({
        "op": "thermalControlProfile.preview",
        "ok": true,
        "target": target_value,
        "previewResponse": thermal_retune_status_summary(&preview_response),
        "statusReadback": thermal_retune_status_summary(&status),
        "error": null,
    });
    fs::write(
        &replay_summary_path,
        serde_json::to_vec_pretty(&replay_summary)?,
    )?;
    Ok(replay_summary)
}

fn thermal_retune_requested_target_value(target: &TargetSelector, default_devd: &str) -> Value {
    json!({
        "deviceId": target.device,
        "hardwareId": target.hardware,
        "devd": default_devd,
    })
}

fn thermal_retune_status_summary(status: &Value) -> Value {
    let mut summary = serde_json::Map::new();
    for key in [
        "deviceId",
        "mode",
        "targetTempC",
        "currentTempC",
        "heaterEnabled",
        "thermalControlProfilePreview",
    ] {
        if let Some(value) = status.get(key) {
            summary.insert(key.to_string(), value.clone());
        }
    }
    if let Some(thermal_control) = status.get("thermalControl") {
        summary.insert("thermalControl".to_string(), thermal_control.clone());
    }
    Value::Object(summary)
}

async fn collect_thermal_self_test(
    client: &Client,
    default_devd: &str,
    args: ThermalSelfTestArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    if args.candidate_profile_files.is_empty() {
        return collect_single_thermal_self_test(client, default_devd, args, true).await;
    }
    if args.seed_profile_file.is_some() {
        return Err(
            "thermal batch self-test cannot combine --seed-profile-file with --candidate-profile-file"
                .into(),
        );
    }
    if !args.skip_optimize {
        return Err("thermal batch self-test requires --skip-optimize".into());
    }
    let target_temps_c = parse_thermal_targets(args.targets_c.as_deref())?;
    if target_temps_c.len() != 1 {
        return Err("thermal batch self-test requires exactly one --targets-c value".into());
    }
    collect_batch_thermal_self_test(client, default_devd, args, target_temps_c[0]).await
}

fn thermal_batch_restart_temp_c(target_temp_c: i16, requested_cooldown_temp_c: f64) -> f64 {
    if (requested_cooldown_temp_c - 40.0).abs() > f64::EPSILON {
        return requested_cooldown_temp_c;
    }
    f64::from((target_temp_c - 30).max(40))
}

fn thermal_source_defaults_for_class(source_class: &str) -> (u16, u16) {
    match source_class {
        "pps5a" => (21_000, 5_000),
        _ => (20_000, 3_250),
    }
}

fn read_thermal_bench_source_class(
    source_kind: BenchSourceKind,
    source_url: &str,
    source_id: &str,
) -> Result<&'static str, Box<dyn std::error::Error + Send + Sync>> {
    match source_kind {
        BenchSourceKind::Isolapurr => read_isolapurr_configured_source_class(source_url, source_id),
    }
}

fn resolve_thermal_source_selection(
    args: &ThermalSelfTestArgs,
) -> Result<ThermalSourceSelection, Box<dyn std::error::Error + Send + Sync>> {
    let detected_source_class = match args.profile_mode {
        ThermalProfileMode::Auto => {
            read_thermal_bench_source_class(args.source_kind, &args.source_url, &args.source_id)?
        }
        ThermalProfileMode::W65 => thermal_source_class(20_000, 3_250),
        ThermalProfileMode::W100 => thermal_source_class(21_000, 5_000),
    };
    let resolved_bank = args
        .profile_mode
        .explicit_bank()
        .unwrap_or(detected_source_class);
    let (default_voltage_mv, default_current_ma) = args
        .profile_mode
        .explicit_source_defaults()
        .unwrap_or_else(|| thermal_source_defaults_for_class(detected_source_class));
    Ok(ThermalSourceSelection {
        resolved_bank,
        detected_source_class,
        detected_source_class_basis: "configured_capability",
        default_voltage_mv,
        default_current_ma,
    })
}

fn thermal_source_request(
    args: &ThermalSelfTestArgs,
    selection: &ThermalSourceSelection,
) -> Result<(u16, u16), Box<dyn std::error::Error + Send + Sync>> {
    let voltage_mv = args
        .source_voltage_v
        .as_deref()
        .map(parse_pps_volts)
        .transpose()?
        .unwrap_or(selection.default_voltage_mv);
    let current_ma = args
        .source_current_a
        .as_deref()
        .map(parse_pps_amps)
        .transpose()?
        .unwrap_or(selection.default_current_ma);
    Ok((voltage_mv, current_ma))
}

async fn collect_batch_thermal_self_test(
    client: &Client,
    default_devd: &str,
    args: ThermalSelfTestArgs,
    target_temp_c: i16,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let resolved = resolve_target(args.target.clone(), default_devd)?;
    let source_selection = resolve_thermal_source_selection(&args)?;
    let (source_voltage_mv, source_current_ma) = thermal_source_request(&args, &source_selection)?;
    let restart_temp_c = thermal_batch_restart_temp_c(target_temp_c, args.cooldown_temp_c);
    let batch_id = format!(
        "thermal-batch-{}-{}",
        current_unix_millis(),
        slugify_path_component(&resolved.device)
    );
    let batch_dir = args.output_dir.join(&batch_id);
    fs::create_dir_all(&batch_dir)?;
    let mut runs = Vec::<Value>::new();
    let mut batch_error = None::<String>;

    if args.dry_run {
        for (candidate_index, candidate_file) in args.candidate_profile_files.iter().enumerate() {
            let imported = serde_json::from_slice::<Value>(&fs::read(candidate_file)?)?;
            let profile =
                thermal_candidate_profile_to_value(&thermal_candidate_profile_from_value(imported));
            let run_id = format!("{batch_id}-candidate-{candidate_index}");
            let run_dir = batch_dir.join(format!("candidate-{candidate_index}"));
            fs::create_dir_all(&run_dir)?;
            let samples_path = run_dir.join("samples.ndjson");
            let mut samples_writer = BufWriter::new(File::create(&samples_path)?);
            let mut sample_index = 0usize;
            let results = write_dry_thermal_ladder(
                &mut samples_writer,
                &run_id,
                "applied",
                source_voltage_mv,
                source_current_ma,
                Some(&profile),
                "preview",
                &[target_temp_c],
                &mut sample_index,
            )?;
            samples_writer.flush()?;
            let validation =
                validate_thermal_applied_results(&results, &[target_temp_c], args.evaluation_mode);
            let mut summary = thermal_batch_candidate_summary(
                &run_id,
                &resolved,
                &args,
                &source_selection,
                candidate_file,
                candidate_index,
                target_temp_c,
                restart_temp_c,
                source_voltage_mv,
                source_current_ma,
                &run_dir,
                &samples_path,
                &profile,
                &results,
                validation,
                sample_index,
            );
            thermal_summary_attach_source_analysis_from_ndjson(&mut summary, &samples_path)?;
            write_thermal_batch_candidate_files(&summary, &run_dir)?;
            runs.push(summary);
        }
    } else {
        validate_thermal_bench_source_tools(args.source_kind)?;
        let (initial_source_telemetry, lease) = prepare_thermal_source_and_lease(
            client,
            &resolved,
            args.source_kind,
            &args.source_url,
            &args.source_id,
            &args.source_mode,
            args.profile_mode,
            args.source_power_watts,
            source_voltage_mv,
            source_current_ma,
        )
        .await?;
        let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
        let test_future = async {
            let mut source_sampler = BenchSourceTelemetrySampler::new(
                args.source_kind,
                &args.source_url,
                initial_source_telemetry,
            );
            for (candidate_index, candidate_file) in args.candidate_profile_files.iter().enumerate()
            {
                let imported = serde_json::from_slice::<Value>(&fs::read(candidate_file)?)?;
                let profile = thermal_candidate_profile_to_value(
                    &thermal_candidate_profile_from_value(imported),
                );
                request_leased(
                    client,
                    &resolved,
                    &lease.lease_id,
                    Method::PUT,
                    "/runtime",
                    Some(thermal_self_test_cooldown_runtime_body()),
                )
                .await?;
                wait_for_cooldown(
                    client,
                    &resolved,
                    &lease.lease_id,
                    restart_temp_c,
                    Duration::from_secs(args.cooldown_timeout_seconds.max(1)),
                )
                .await?;
                let run_id = format!("{batch_id}-candidate-{candidate_index}");
                let run_dir = batch_dir.join(format!("candidate-{candidate_index}"));
                fs::create_dir_all(&run_dir)?;
                let samples_path = run_dir.join("samples.ndjson");
                let mut samples_writer = BufWriter::new(File::create(&samples_path)?);
                let mut sample_index = 0usize;
                let heater_parameters =
                    thermal_heater_parameters_value(target_temp_c, Some(&profile), "preview");
                preview_and_prepare_thermal_self_test_target(
                    client,
                    &resolved,
                    &lease.lease_id,
                    args.profile_mode,
                    &profile,
                    target_temp_c,
                    &heater_parameters,
                )
                .await?;
                source_sampler.refresh().await?;
                let arm_status =
                    arm_thermal_self_test_target(client, &resolved, &lease.lease_id, target_temp_c)
                        .await?;
                let result = run_thermal_stage(
                    client,
                    &resolved,
                    &lease.lease_id,
                    &mut samples_writer,
                    &run_id,
                    "applied",
                    target_temp_c,
                    source_voltage_mv,
                    source_current_ma,
                    &heater_parameters,
                    &args,
                    &mut source_sampler,
                    &mut sample_index,
                    Some(arm_status),
                )
                .await?;
                let _ = arm_thermal_self_test_heater(
                    client,
                    &resolved,
                    &lease.lease_id,
                    false,
                    target_temp_c,
                )
                .await?;
                samples_writer.flush()?;
                let results = vec![result];
                let validation = validate_thermal_applied_results(
                    &results,
                    &[target_temp_c],
                    args.evaluation_mode,
                );
                let mut summary = thermal_batch_candidate_summary(
                    &run_id,
                    &resolved,
                    &args,
                    &source_selection,
                    candidate_file,
                    candidate_index,
                    target_temp_c,
                    restart_temp_c,
                    source_voltage_mv,
                    source_current_ma,
                    &run_dir,
                    &samples_path,
                    &profile,
                    &results,
                    validation,
                    sample_index,
                );
                thermal_summary_attach_source_analysis_from_ndjson(&mut summary, &samples_path)?;
                write_thermal_batch_candidate_files(&summary, &run_dir)?;
                runs.push(summary);
            }
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        };
        let test_result = tokio::select! {
            result = test_future => result,
            signal = tokio::signal::ctrl_c() => {
                signal?;
                Err("thermal batch self-test interrupted; heater cleanup requested".into())
            }
        };
        heartbeat.abort();
        if let Err(error) = test_result {
            batch_error = Some(error.to_string());
        }
        if let Err(error) =
            force_thermal_self_test_shutdown(client, &resolved, &lease.lease_id).await
            && batch_error.is_none()
        {
            batch_error = Some(format!("thermal batch cleanup failed: {error}"));
        }
        let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
        if let Err(error) = set_thermal_bench_source_output_auto(
            client,
            args.source_kind,
            &args.source_url,
            &args.source_id,
        )
        .await
            && batch_error.is_none()
        {
            batch_error = Some(format!(
                "{} cleanup failed: {error}",
                args.source_kind.as_str()
            ));
        }
    }

    let passed_candidates = runs
        .iter()
        .filter(|run| run.pointer("/validation/passed").and_then(Value::as_bool) == Some(true))
        .count();
    let summary = json!({
        "kind": "thermal_self_test_batch",
        "ok": batch_error.is_none() && passed_candidates > 0 && runs.len() == args.candidate_profile_files.len(),
        "batchId": batch_id,
        "targetTempC": target_temp_c,
        "restartTempC": restart_temp_c,
        "candidateCount": args.candidate_profile_files.len(),
        "completedCandidateCount": runs.len(),
        "passedCandidateCount": passed_candidates,
        "profilePersistence": "not_saved",
        "sourceHeldAcrossCandidates": !args.dry_run,
        "evaluationMode": args.evaluation_mode.as_str(),
        "runs": runs,
        "error": batch_error,
    });
    fs::write(
        batch_dir.join("batch.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
fn thermal_batch_candidate_summary(
    run_id: &str,
    resolved: &ResolvedUsbTarget,
    args: &ThermalSelfTestArgs,
    source_selection: &ThermalSourceSelection,
    candidate_file: &Path,
    candidate_index: usize,
    target_temp_c: i16,
    restart_temp_c: f64,
    source_voltage_mv: u16,
    source_current_ma: u16,
    run_dir: &Path,
    samples_path: &Path,
    profile: &Value,
    results: &[ThermalStageResult],
    validation: Value,
    sample_count: usize,
) -> Value {
    let complete = validation.get("passed").and_then(Value::as_bool) == Some(true);
    json!({
        "kind": "thermal_self_test",
        "ok": complete,
        "runId": run_id,
        "dryRun": args.dry_run,
        "batchCandidateIndex": candidate_index,
        "target": {
            "deviceId": resolved.device,
            "hardwareId": resolved.hardware_id,
            "devd": resolved.devd,
        },
        "source": thermal_source_summary_value(
            args,
            source_selection,
            source_voltage_mv,
            source_current_ma,
        ),
        "parameters": {
            "targetsC": [target_temp_c],
            "candidateProfileFile": candidate_file,
            "evaluationMode": args.evaluation_mode.as_str(),
            "sampleIntervalMs": args.sample_interval_ms.max(1),
            "effectiveSampleIntervalMs": effective_thermal_sample_interval_ms(args.sample_interval_ms),
            "holdSeconds": args.hold_seconds.max(1),
            "stageTimeoutSeconds": args.stage_timeout_seconds.max(1),
            "warmupTimeoutSeconds": args.warmup_timeout_seconds.max(1),
            "runtimeRearmAttempts": args.runtime_rearm_attempts,
            "cooldownTempC": restart_temp_c,
            "cooldownTimeoutSeconds": args.cooldown_timeout_seconds.max(1),
            "batchCandidate": true,
            "limits": {
                "overshootC": 3.0,
                "holdPeakToPeakC": 3.0,
                "minimumSampleRateHz": THERMAL_MIN_SAMPLE_RATE_HZ,
                "sampleRateWindowMs": THERMAL_SAMPLE_RATE_WINDOW_MS,
                "fullSpeedToStableMsByTarget": {
                    "lte150C": ThermalFullSpeedStableTracker::LOW_TEMP_SETTLE_LIMIT_MS,
                    "gt150C": ThermalFullSpeedStableTracker::HIGH_TEMP_SETTLE_LIMIT_MS,
                },
                "fullSpeedStableBandC": ThermalFullSpeedStableTracker::STABLE_BAND_C,
                "fullSpeedStableWindowMs": ThermalFullSpeedStableTracker::STABLE_WINDOW_MS,
                "fullSpeedToStableHardGate": args.evaluation_mode.enforces_stage_limits(),
                "approachThresholdTimeoutMs": 10_000,
                "approachHoldTimeoutMs": 30_000,
                "approachWarmupReentry": "fail"
            }
        },
        "files": {
            "runDir": run_dir,
            "summaryPath": run_dir.join("run.json"),
            "samplesPath": samples_path,
            "candidateProfilePath": run_dir.join("thermal-profile.candidate.json"),
        },
        "candidateProfile": profile,
        "profilePersistence": if args.dry_run { "dry_run" } else { "not_saved" },
        "tuningSteps": [],
        "applied": results.iter().map(ThermalStageResult::to_value).collect::<Vec<_>>(),
        "validation": validation,
        "sampleCount": sample_count,
        "complete": complete,
        "error": null,
    })
}

fn write_thermal_batch_candidate_files(
    summary: &Value,
    run_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    fs::write(
        run_dir.join("thermal-profile.candidate.json"),
        serde_json::to_vec_pretty(&summary["candidateProfile"])?,
    )?;
    fs::write(
        run_dir.join("run.json"),
        serde_json::to_vec_pretty(summary)?,
    )?;
    Ok(())
}

async fn collect_single_thermal_self_test(
    client: &Client,
    default_devd: &str,
    args: ThermalSelfTestArgs,
    save_profile_on_pass: bool,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let resolved = resolve_target(args.target.clone(), default_devd)?;
    let source_selection = resolve_thermal_source_selection(&args)?;
    let (source_voltage_mv, source_current_ma) = thermal_source_request(&args, &source_selection)?;
    let target_temps_c = parse_thermal_targets(args.targets_c.as_deref())?;
    let optimize_targets_c = if args.skip_optimize {
        Vec::new()
    } else {
        resolve_optimization_targets(&target_temps_c, args.optimize_targets_c.as_deref())?
    };
    let (mut candidate_profile, effective_seed_profile_file) =
        if let Some(seed_profile_file) = args.seed_profile_file.as_ref() {
            (
                thermal_candidate_profile_from_value(serde_json::from_slice(&fs::read(
                    seed_profile_file,
                )?)?),
                Some(seed_profile_file.clone()),
            )
        } else {
            load_thermal_default_seed_candidate_profile(source_selection.resolved_bank)?
        };
    let mut candidate_profile_value = thermal_candidate_profile_to_value(&candidate_profile);
    let run_started_unix_ms = current_unix_millis();
    let run_id = format!(
        "thermal-{}-{}",
        run_started_unix_ms,
        slugify_path_component(&resolved.device)
    );
    let run_dir = args.output_dir.join(&run_id);
    fs::create_dir_all(&run_dir)?;
    let samples_path = run_dir.join("samples.ndjson");
    let summary_path = run_dir.join("run.json");
    let candidate_path = run_dir.join("thermal-profile.candidate.json");
    fs::write(
        &candidate_path,
        serde_json::to_vec_pretty(&candidate_profile_value)?,
    )?;
    let mut samples_writer = BufWriter::new(File::create(&samples_path)?);

    let mut sample_index = 0usize;
    let mut applied_results = Vec::new();
    let mut run_error = None::<String>;
    let mut saved_profile_retained = false;
    let mut tuning_steps = Vec::<Value>::new();

    if args.dry_run {
        applied_results = write_dry_thermal_ladder(
            &mut samples_writer,
            &run_id,
            "applied",
            source_voltage_mv,
            source_current_ma,
            Some(&candidate_profile_value),
            "saved",
            &target_temps_c,
            &mut sample_index,
        )?;
    } else {
        validate_thermal_bench_source_tools(args.source_kind)?;
        let (initial_source_telemetry, lease) = prepare_thermal_source_and_lease(
            client,
            &resolved,
            args.source_kind,
            &args.source_url,
            &args.source_id,
            &args.source_mode,
            args.profile_mode,
            args.source_power_watts,
            source_voltage_mv,
            source_current_ma,
        )
        .await?;
        let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());

        let test_future = async {
            let mut source_sampler = BenchSourceTelemetrySampler::new(
                args.source_kind,
                &args.source_url,
                initial_source_telemetry,
            );
            request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::PUT,
                "/runtime",
                Some(thermal_self_test_cooldown_runtime_body()),
            )
            .await?;
            wait_for_cooldown(
                client,
                &resolved,
                &lease.lease_id,
                args.cooldown_temp_c,
                Duration::from_secs(args.cooldown_timeout_seconds.max(1)),
            )
            .await?;
            let mut optimization_completed = true;
            for (stage_index, target_temp_c) in optimize_targets_c.iter().copied().enumerate() {
                let heater_parameters = thermal_heater_parameters_value(
                    target_temp_c,
                    Some(&candidate_profile_value),
                    "preview",
                );
                preview_and_prepare_thermal_self_test_target(
                    client,
                    &resolved,
                    &lease.lease_id,
                    args.profile_mode,
                    &candidate_profile_value,
                    target_temp_c,
                    &heater_parameters,
                )
                .await?;
                source_sampler.refresh().await?;
                let arm_status =
                    arm_thermal_self_test_target(client, &resolved, &lease.lease_id, target_temp_c)
                        .await?;
                let result = run_thermal_stage(
                    client,
                    &resolved,
                    &lease.lease_id,
                    &mut samples_writer,
                    &run_id,
                    "optimize",
                    target_temp_c,
                    source_voltage_mv,
                    source_current_ma,
                    &heater_parameters,
                    &args,
                    &mut source_sampler,
                    &mut sample_index,
                    Some(arm_status),
                )
                .await?;
                if let Some(point) =
                    thermal_candidate_point_mut(&mut candidate_profile, target_temp_c)
                {
                    *point = tune_thermal_candidate_point(*point, &result);
                }
                thermal_rebuild_profile_from_anchor_targets(
                    &mut candidate_profile,
                    &optimize_targets_c,
                );
                candidate_profile_value = thermal_candidate_profile_to_value(&candidate_profile);
                fs::write(
                    &candidate_path,
                    serde_json::to_vec_pretty(&candidate_profile_value)?,
                )?;
                tuning_steps.push(json!({
                    "phase": "optimize",
                    "stageIndex": stage_index,
                    "targetTempC": target_temp_c,
                    "result": result.to_value(),
                    "candidateProfile": candidate_profile_value.clone(),
                }));
                let _ = arm_thermal_self_test_heater(
                    client,
                    &resolved,
                    &lease.lease_id,
                    false,
                    target_temp_c,
                )
                .await?;
                if !thermal_stage_can_continue_tuning(&result) {
                    optimization_completed = false;
                    break;
                }
            }
            if optimization_completed {
                if !optimize_targets_c.is_empty() {
                    request_leased(
                        client,
                        &resolved,
                        &lease.lease_id,
                        Method::PUT,
                        "/runtime",
                        Some(thermal_self_test_cooldown_runtime_body()),
                    )
                    .await?;
                    wait_for_cooldown(
                        client,
                        &resolved,
                        &lease.lease_id,
                        args.cooldown_temp_c,
                        Duration::from_secs(args.cooldown_timeout_seconds.max(1)),
                    )
                    .await?;
                }
                for (stage_index, target_temp_c) in target_temps_c.iter().copied().enumerate() {
                    let heater_parameters = thermal_heater_parameters_value(
                        target_temp_c,
                        Some(&candidate_profile_value),
                        "preview",
                    );
                    preview_and_prepare_thermal_self_test_target(
                        client,
                        &resolved,
                        &lease.lease_id,
                        args.profile_mode,
                        &candidate_profile_value,
                        target_temp_c,
                        &heater_parameters,
                    )
                    .await?;
                    source_sampler.refresh().await?;
                    let arm_status = arm_thermal_self_test_target(
                        client,
                        &resolved,
                        &lease.lease_id,
                        target_temp_c,
                    )
                    .await?;
                    let result = run_thermal_stage(
                        client,
                        &resolved,
                        &lease.lease_id,
                        &mut samples_writer,
                        &run_id,
                        "applied",
                        target_temp_c,
                        source_voltage_mv,
                        source_current_ma,
                        &heater_parameters,
                        &args,
                        &mut source_sampler,
                        &mut sample_index,
                        Some(arm_status),
                    )
                    .await?;
                    let _ = arm_thermal_self_test_heater(
                        client,
                        &resolved,
                        &lease.lease_id,
                        false,
                        target_temp_c,
                    )
                    .await?;
                    applied_results.push(result.clone());
                    tuning_steps.push(json!({
                        "phase": "applied",
                        "stageIndex": stage_index,
                        "targetTempC": target_temp_c,
                        "result": result.to_value(),
                        "candidateProfile": candidate_profile_value.clone(),
                    }));
                    if result.stop_reason != "completed" {
                        break;
                    }
                }
            }
            let validation_passed = validate_thermal_applied_results(
                &applied_results,
                &target_temps_c,
                args.evaluation_mode,
            )["passed"]
                .as_bool()
                == Some(true);
            if validation_passed && save_profile_on_pass {
                let persisted_profile_value =
                    thermal_candidate_profile_to_value(&thermal_persisted_profile_for_bank(
                        &candidate_profile,
                        source_selection.resolved_bank,
                    ));
                let save_status = request_leased(
                    client,
                    &resolved,
                    &lease.lease_id,
                    Method::PUT,
                    "/runtime",
                    Some(json!({
                        "thermalControlProfile": {
                            "op": "save",
                            "bank": source_selection.resolved_bank,
                            "profile": persisted_profile_value,
                        },
                        "thermalProfileMode": args.profile_mode.as_str(),
                    })),
                )
                .await?;
                if require_status_bool(&save_status, "thermalControlProfilePreview")? {
                    return Err(
                        "thermal profile save unexpectedly left preview mode enabled".into(),
                    );
                }
                saved_profile_retained = true;
            }
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        };

        let test_result = tokio::select! {
            result = test_future => result,
            signal = tokio::signal::ctrl_c() => {
                signal?;
                Err("thermal self-test interrupted; heater cleanup requested".into())
            }
        };

        heartbeat.abort();
        if let Err(error) = test_result {
            run_error = Some(error.to_string());
        }
        if let Err(error) =
            force_thermal_self_test_shutdown(client, &resolved, &lease.lease_id).await
            && run_error.is_none()
        {
            run_error = Some(format!("thermal self-test cleanup failed: {error}"));
        }
        let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
        if let Err(error) = set_thermal_bench_source_output_auto(
            client,
            args.source_kind,
            &args.source_url,
            &args.source_id,
        )
        .await
            && run_error.is_none()
        {
            run_error = Some(format!(
                "{} cleanup failed: {error}",
                args.source_kind.as_str()
            ));
        }
    }

    samples_writer.flush()?;
    let validation =
        validate_thermal_applied_results(&applied_results, &target_temps_c, args.evaluation_mode);
    let complete = run_error.is_none() && validation["passed"].as_bool() == Some(true);
    let mut summary = json!({
        "kind": "thermal_self_test",
        "ok": complete,
        "runId": run_id,
        "dryRun": args.dry_run,
        "target": {
            "deviceId": resolved.device,
            "hardwareId": resolved.hardware_id,
            "devd": resolved.devd,
        },
        "source": thermal_source_summary_value(
            &args,
            &source_selection,
            source_voltage_mv,
            source_current_ma,
        ),
        "parameters": {
            "targetsC": target_temps_c,
            "optimizeTargetsC": optimize_targets_c,
            "seedProfileFile": effective_seed_profile_file
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            "batchCandidate": !save_profile_on_pass,
            "evaluationMode": args.evaluation_mode.as_str(),
            "sampleIntervalMs": args.sample_interval_ms.max(1),
            "effectiveSampleIntervalMs": effective_thermal_sample_interval_ms(args.sample_interval_ms),
            "holdSeconds": args.hold_seconds.max(1),
            "stageTimeoutSeconds": args.stage_timeout_seconds.max(1),
            "warmupTimeoutSeconds": args.warmup_timeout_seconds.max(1),
            "runtimeRearmAttempts": args.runtime_rearm_attempts,
            "cooldownTempC": args.cooldown_temp_c,
            "cooldownTimeoutSeconds": args.cooldown_timeout_seconds.max(1),
            "limits": {
                "overshootC": 3.0,
                "holdPeakToPeakC": 3.0,
                "minimumSampleRateHz": THERMAL_MIN_SAMPLE_RATE_HZ,
                "sampleRateWindowMs": THERMAL_SAMPLE_RATE_WINDOW_MS,
                "fullSpeedToStableMsByTarget": {
                    "lte150C": ThermalFullSpeedStableTracker::LOW_TEMP_SETTLE_LIMIT_MS,
                    "gt150C": ThermalFullSpeedStableTracker::HIGH_TEMP_SETTLE_LIMIT_MS,
                },
                "fullSpeedStableBandC": ThermalFullSpeedStableTracker::STABLE_BAND_C,
                "fullSpeedStableWindowMs": ThermalFullSpeedStableTracker::STABLE_WINDOW_MS,
                "fullSpeedToStableHardGate": args.evaluation_mode.enforces_stage_limits(),
                "approachThresholdTimeoutMs": 10_000,
                "approachHoldTimeoutMs": 30_000,
                "approachWarmupReentry": "fail"
            }
        },
        "files": {
            "runDir": run_dir,
            "summaryPath": summary_path,
            "samplesPath": samples_path,
            "candidateProfilePath": candidate_path,
        },
        "candidateProfile": candidate_profile_value.clone(),
        "profilePersistence": if args.dry_run {
            "dry_run"
        } else if saved_profile_retained {
            "saved_tuned_candidate"
        } else {
            "not_saved"
        },
        "tuningSteps": tuning_steps,
        "applied": applied_results.iter().map(ThermalStageResult::to_value).collect::<Vec<_>>(),
        "validation": validation,
        "sampleCount": sample_index,
        "complete": complete,
        "error": run_error,
    });
    thermal_summary_attach_source_analysis_from_ndjson(&mut summary, &samples_path)?;
    fs::write(&summary_path, serde_json::to_vec_pretty(&summary)?)?;
    if let Some(id) = summary
        .get("target")
        .and_then(|target| target.get("hardwareId"))
        .and_then(Value::as_str)
    {
        let _ = remember_usb(
            id,
            summary["target"]["deviceId"].as_str().unwrap_or_default(),
            summary["target"]["devd"]
                .as_str()
                .unwrap_or(DEFAULT_DEVD_URL),
        );
    }
    Ok(summary)
}

#[cfg(test)]
fn default_thermal_candidate_profile() -> Value {
    thermal_candidate_profile_to_value(&thermal_seed_candidate_profile())
}

const THERMAL_PPS3A_ACCEPTED_SEED_RELATIVE: &str = "thermal-self-test-runs/baselines/56x56mm-3p2ohm-pd63w-pps3a/accepted-full-range-20hz/thermal-profile.accepted.json";
const THERMAL_PPS5A_ACCEPTED_SEED_RELATIVE: &str = "thermal-self-test-runs/baselines/56x56mm-3p2ohm-pd100w-pps5a/accepted-full-range-20hz/thermal-profile.accepted.json";
const THERMAL_PPS5A_TUNING_SEED_RELATIVE: &str =
    "thermal-self-test-runs/variants_100c_v6_hold220_cutoff90.json";

fn flux_purr_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .to_path_buf()
}

fn thermal_default_seed_candidates_for_bank(bank: &str) -> Vec<PathBuf> {
    let repo_root = flux_purr_repo_root();
    match bank {
        "pps5a" => vec![
            repo_root.join(THERMAL_PPS5A_ACCEPTED_SEED_RELATIVE),
            repo_root.join(THERMAL_PPS5A_TUNING_SEED_RELATIVE),
        ],
        "pps3a" => vec![repo_root.join(THERMAL_PPS3A_ACCEPTED_SEED_RELATIVE)],
        _ => Vec::new(),
    }
}

fn load_thermal_default_seed_candidate_profile(
    bank: &str,
) -> Result<(ThermalCandidateProfile, Option<PathBuf>), Box<dyn std::error::Error + Send + Sync>> {
    for path in thermal_default_seed_candidates_for_bank(bank) {
        if path.exists() {
            let imported = serde_json::from_slice::<Value>(&fs::read(&path)?)?;
            return Ok((thermal_candidate_profile_from_value(imported), Some(path)));
        }
    }
    Ok((thermal_seed_candidate_profile(), None))
}

fn thermal_seed_candidate_profile() -> ThermalCandidateProfile {
    ThermalCandidateProfile {
        settings: thermal_default_settings(),
        points: THERMAL_PROFILE_ANCHOR_TARGETS_C
            .iter()
            .copied()
            .map(thermal_default_target_point)
            .collect(),
    }
}

fn thermal_persisted_targets_for_bank(bank: &str) -> &'static [i16] {
    match bank {
        "pps5a" => &THERMAL_PPS5A_PERSISTED_TARGETS_C,
        _ => &THERMAL_PPS3A_PERSISTED_TARGETS_C,
    }
}

fn thermal_persisted_profile_for_bank(
    profile: &ThermalCandidateProfile,
    bank: &str,
) -> ThermalCandidateProfile {
    let points = thermal_persisted_targets_for_bank(bank)
        .iter()
        .filter_map(|target_temp_c| thermal_interpolated_candidate_point(profile, *target_temp_c))
        .collect::<Vec<_>>();
    ThermalCandidateProfile {
        settings: profile.settings,
        points,
    }
}

fn thermal_default_settings() -> ThermalCandidateSettings {
    ThermalCandidateSettings {
        temp_filter_alpha_permille: 750,
        // A noisy sub-second reading must not re-enter full-power warmup after the controller
        // has started braking. The threshold remains profile-controlled and is persisted with
        // the rest of the adaptive point data.
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 20,
        hold_exit_centi_c: 80,
        hold_on_centi_c: 15,
        hold_off_centi_c: 80,
        overshoot_cutoff_centi_c: 120,
        approach_max_ticks: 250,
        approach_min_power_ratio_permille: 500,
        hold_kp_permille_per_c: 35,
        hold_ki_permille_per_c_tick: 1,
        hold_blend_ticks: 12,
        hold_reheat_power_permille: 0,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
        auto_adjustable_working_floor_mv: 5_000,
        heater_current_reserve_ma: 200,
    }
}

fn thermal_default_target_point(target_temp_c: i16) -> ThermalCandidatePoint {
    let (
        brake_distance_centi_c,
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        hold_power_permille,
        hold_reheat_power_permille,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks,
    ) = thermal_default_target_values(target_temp_c);
    ThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c,
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        approach_tail_window_centi_c: 0,
        hold_power_permille,
        hold_reheat_power_permille,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks,
    }
}

fn thermal_candidate_profile_to_value(profile: &ThermalCandidateProfile) -> Value {
    let mut points = profile
        .points
        .iter()
        .copied()
        .map(|point| {
            json!({
                "targetTempC": point.target_temp_c,
                "brakeDistanceCentiC": point.brake_distance_centi_c,
                "warmupPowerPermille": 1_000,
                "approachPowerPermille": point.approach_power_permille,
                "approachFloorPowerPermille": point.approach_floor_power_permille,
                "approachDampingExponentPermille": point.approach_damping_exponent_permille,
                "approachTailWindowCentiC": point.approach_tail_window_centi_c,
                "holdPowerPermille": point.hold_power_permille,
                "holdReheatPowerPermille": point.hold_reheat_power_permille,
                "holdEntryCentiC": point.hold_entry_centi_c,
                "holdExitCentiC": point.hold_exit_centi_c,
                "holdOnCentiC": point.hold_on_centi_c,
                "holdOffCentiC": point.hold_off_centi_c,
                "overshootCutoffCentiC": point.overshoot_cutoff_centi_c,
                "holdKpPermillePerC": point.hold_kp_permille_per_c,
                "holdKiPermillePerCTick": point.hold_ki_permille_per_c_tick,
                "holdBlendTicks": point.hold_blend_ticks,
                "approachLeadTicks": point.approach_lead_ticks,
                "holdLeadTicks": point.hold_lead_ticks,
            })
        })
        .collect::<Vec<_>>();
    while points.len() < THERMAL_CONTROL_PROFILE_MAX_POINTS {
        points.push(Value::Null);
    }
    json!({
        "settings": thermal_candidate_settings_to_value(profile.settings),
        "points": points
    })
}

fn thermal_candidate_profile_from_value(imported: Value) -> ThermalCandidateProfile {
    let profile = thermal_profile_package_from_value(imported);
    let settings_value = profile.get("settings").cloned().unwrap_or(Value::Null);
    let default_settings = thermal_default_settings();
    let settings = ThermalCandidateSettings {
        temp_filter_alpha_permille: settings_value
            .get("tempFilterAlphaPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.temp_filter_alpha_permille),
        warmup_reenter_centi_c: settings_value
            .get("warmupReenterCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.warmup_reenter_centi_c),
        hold_entry_centi_c: settings_value
            .get("holdEntryCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.hold_entry_centi_c),
        hold_exit_centi_c: settings_value
            .get("holdExitCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.hold_exit_centi_c),
        hold_on_centi_c: settings_value
            .get("holdOnCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.hold_on_centi_c),
        hold_off_centi_c: settings_value
            .get("holdOffCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.hold_off_centi_c),
        overshoot_cutoff_centi_c: settings_value
            .get("overshootCutoffCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.overshoot_cutoff_centi_c),
        approach_max_ticks: settings_value
            .get("approachMaxTicks")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.approach_max_ticks),
        approach_min_power_ratio_permille: settings_value
            .get("approachMinPowerRatioPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.approach_min_power_ratio_permille),
        hold_kp_permille_per_c: settings_value
            .get("holdKpPermillePerC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.hold_kp_permille_per_c),
        hold_ki_permille_per_c_tick: settings_value
            .get("holdKiPermillePerCTick")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.hold_ki_permille_per_c_tick),
        hold_blend_ticks: settings_value
            .get("holdBlendTicks")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.hold_blend_ticks),
        hold_reheat_power_permille: settings_value
            .get("holdReheatPowerPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.hold_reheat_power_permille),
        approach_lead_ticks: settings_value
            .get("approachLeadTicks")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.approach_lead_ticks),
        hold_lead_ticks: settings_value
            .get("holdLeadTicks")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.hold_lead_ticks),
        auto_adjustable_working_floor_mv: settings_value
            .get("autoAdjustableWorkingFloorMv")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.auto_adjustable_working_floor_mv),
        heater_current_reserve_ma: settings_value
            .get("heaterCurrentReserveMa")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_settings.heater_current_reserve_ma),
    };
    let points_value = profile
        .get("points")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let point_targets = if points_value.is_empty() {
        THERMAL_PROFILE_ANCHOR_TARGETS_C.to_vec()
    } else {
        points_value
            .iter()
            .filter_map(|point| {
                point
                    .get("targetTempC")
                    .and_then(Value::as_i64)
                    .and_then(|value| i16::try_from(value).ok())
            })
            .collect::<Vec<_>>()
    };
    let points = point_targets
        .into_iter()
        .map(|target_temp_c| {
            let default_point = thermal_default_target_point(target_temp_c);
            let point_value = points_value.iter().find(|point| {
                point
                    .get("targetTempC")
                    .and_then(Value::as_i64)
                    .is_some_and(|value| value == i64::from(target_temp_c))
            });
            let point_value = point_value.cloned().unwrap_or(Value::Null);
            let mut point = ThermalCandidatePoint {
                target_temp_c,
                brake_distance_centi_c: point_value
                    .get("brakeDistanceCentiC")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.brake_distance_centi_c),
                warmup_power_permille: 1_000,
                approach_power_permille: point_value
                    .get("approachPowerPermille")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.approach_power_permille),
                approach_floor_power_permille: point_value
                    .get("approachFloorPowerPermille")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.approach_floor_power_permille),
                approach_damping_exponent_permille: point_value
                    .get("approachDampingExponentPermille")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.approach_damping_exponent_permille),
                approach_tail_window_centi_c: point_value
                    .get("approachTailWindowCentiC")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.approach_tail_window_centi_c),
                hold_power_permille: point_value
                    .get("holdPowerPermille")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.hold_power_permille),
                hold_reheat_power_permille: point_value
                    .get("holdReheatPowerPermille")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.hold_reheat_power_permille),
                hold_entry_centi_c: point_value
                    .get("holdEntryCentiC")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.hold_entry_centi_c),
                hold_exit_centi_c: point_value
                    .get("holdExitCentiC")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.hold_exit_centi_c),
                hold_on_centi_c: point_value
                    .get("holdOnCentiC")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(settings.hold_on_centi_c),
                hold_off_centi_c: point_value
                    .get("holdOffCentiC")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.hold_off_centi_c),
                overshoot_cutoff_centi_c: point_value
                    .get("overshootCutoffCentiC")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.overshoot_cutoff_centi_c),
                hold_kp_permille_per_c: point_value
                    .get("holdKpPermillePerC")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.hold_kp_permille_per_c),
                hold_ki_permille_per_c_tick: point_value
                    .get("holdKiPermillePerCTick")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.hold_ki_permille_per_c_tick),
                hold_blend_ticks: point_value
                    .get("holdBlendTicks")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.hold_blend_ticks),
                approach_lead_ticks: point_value
                    .get("approachLeadTicks")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.approach_lead_ticks),
                hold_lead_ticks: point_value
                    .get("holdLeadTicks")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16)
                    .unwrap_or(default_point.hold_lead_ticks),
            };
            point.warmup_power_permille = 1_000;
            point
        })
        .collect();
    ThermalCandidateProfile { settings, points }
}

fn thermal_candidate_settings_to_value(settings: ThermalCandidateSettings) -> Value {
    json!({
        "tempFilterAlphaPermille": settings.temp_filter_alpha_permille,
        "warmupReenterCentiC": settings.warmup_reenter_centi_c,
        "holdEntryCentiC": settings.hold_entry_centi_c,
        "holdExitCentiC": settings.hold_exit_centi_c,
        "holdOnCentiC": settings.hold_on_centi_c,
        "holdOffCentiC": settings.hold_off_centi_c,
        "overshootCutoffCentiC": settings.overshoot_cutoff_centi_c,
        "approachMaxTicks": settings.approach_max_ticks,
        "approachMinPowerRatioPermille": settings.approach_min_power_ratio_permille,
        "holdKpPermillePerC": settings.hold_kp_permille_per_c,
        "holdKiPermillePerCTick": settings.hold_ki_permille_per_c_tick,
        "holdBlendTicks": settings.hold_blend_ticks,
        "holdReheatPowerPermille": settings.hold_reheat_power_permille,
        "approachLeadTicks": settings.approach_lead_ticks,
        "holdLeadTicks": settings.hold_lead_ticks,
        "autoAdjustableWorkingFloorMv": settings.auto_adjustable_working_floor_mv,
        "heaterCurrentReserveMa": settings.heater_current_reserve_ma,
    })
}

fn effective_thermal_sample_interval_ms(requested_ms: u64) -> u64 {
    requested_ms.clamp(1, 300)
}

const THERMAL_MIN_SAMPLE_RATE_HZ: f64 = 3.0;
const THERMAL_SAMPLE_RATE_WINDOW_MS: u64 = 3_000;
const THERMAL_SAMPLE_RATE_FAILURE_GRACE_MS: u64 = 3_000;
const THERMAL_HEATER_OUTPUT_START_TIMEOUT_MS: u64 = 2_000;
const THERMAL_COOLDOWN_POLL_INTERVAL_MS: u64 = 1_000;

impl ThermalSampleRateTracker {
    fn new() -> Self {
        Self {
            elapsed_ms: Vec::with_capacity(32),
            below_minimum_since_ms: None,
        }
    }

    fn observe(&mut self, elapsed_ms: u64) -> ThermalSampleRateObservation {
        let interval_ms = self
            .elapsed_ms
            .last()
            .map(|previous| elapsed_ms.saturating_sub(*previous));
        self.elapsed_ms.push(elapsed_ms);
        let cutoff_ms = elapsed_ms.saturating_sub(THERMAL_SAMPLE_RATE_WINDOW_MS);
        self.elapsed_ms.retain(|sample_ms| *sample_ms >= cutoff_ms);
        let rolling_rate_hz = if elapsed_ms >= THERMAL_SAMPLE_RATE_WINDOW_MS {
            Some(
                self.elapsed_ms.len().saturating_sub(1) as f64 * 1_000.0
                    / THERMAL_SAMPLE_RATE_WINDOW_MS as f64,
            )
        } else {
            None
        };
        let below_minimum =
            rolling_rate_hz.is_some_and(|rate_hz| rate_hz < THERMAL_MIN_SAMPLE_RATE_HZ);
        if below_minimum {
            self.below_minimum_since_ms.get_or_insert(elapsed_ms);
        } else {
            self.below_minimum_since_ms = None;
        }
        let violation = self.below_minimum_since_ms.is_some_and(|started_at_ms| {
            elapsed_ms.saturating_sub(started_at_ms) >= THERMAL_SAMPLE_RATE_FAILURE_GRACE_MS
        });
        ThermalSampleRateObservation {
            interval_ms,
            rolling_rate_hz,
            violation,
        }
    }
}

fn thermal_default_target_values(
    target_temp_c: i16,
) -> (
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
) {
    if target_temp_c <= 60 {
        (
            // The verified low-temperature point predicts stored heat during Approach, enters
            // Hold early, then coasts at zero output until the plate starts falling again.
            1_310, 1_000, 590, 510, 1_320, 60, 60, 200, 540, 30, 120, 150, 8, 2, 1, 4, 2,
        )
    } else if target_temp_c <= 100 {
        (
            1_100, 1_000, 420, 220, 1_400, 170, 260, 12, 60, 30, 180, 230, 55, 2, 6, 9, 0,
        )
    } else if target_temp_c <= 140 {
        (
            1_000, 1_000, 420, 200, 1_000, 280, 340, 10, 55, 30, 160, 220, 22, 1, 1, 4, 0,
        )
    } else if target_temp_c <= 180 {
        (
            650, 1_000, 760, 460, 800, 450, 620, 15, 70, 25, 240, 300, 20, 1, 3, 4, 0,
        )
    } else if target_temp_c <= 220 {
        (
            520, 1_000, 760, 600, 550, 620, 700, 8, 50, 14, 240, 320, 22, 1, 2, 2, 0,
        )
    } else {
        (
            500, 1_000, 960, 860, 350, 850, 930, 10, 55, 14, 320, 420, 12, 1, 1, 1, 0,
        )
    }
}

fn thermal_default_settings_value() -> Value {
    thermal_candidate_settings_to_value(thermal_default_settings())
}

fn resolve_optimization_targets(
    requested_targets_c: &[i16],
    optimize_targets_c: Option<&str>,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(optimize_targets_c) = optimize_targets_c {
        return parse_thermal_targets(Some(optimize_targets_c));
    }
    if requested_targets_c.len() <= 3 {
        return Ok(requested_targets_c.to_vec());
    }
    let min_target = requested_targets_c[0];
    let max_target = *requested_targets_c.last().unwrap_or(&min_target);
    let midpoint = (f64::from(min_target) + f64::from(max_target)) / 2.0;
    let middle_target = requested_targets_c[1..requested_targets_c.len() - 1]
        .iter()
        .copied()
        .min_by(|left, right| {
            (f64::from(*left) - midpoint)
                .abs()
                .partial_cmp(&(f64::from(*right) - midpoint).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(min_target);
    Ok(vec![min_target, middle_target, max_target])
}

fn thermal_candidate_point(
    profile: &ThermalCandidateProfile,
    target_temp_c: i16,
) -> Option<ThermalCandidatePoint> {
    profile
        .points
        .iter()
        .copied()
        .find(|point| point.target_temp_c == target_temp_c)
}

fn thermal_interpolated_candidate_point(
    profile: &ThermalCandidateProfile,
    target_temp_c: i16,
) -> Option<ThermalCandidatePoint> {
    if let Some(point) = thermal_candidate_point(profile, target_temp_c) {
        return Some(point);
    }
    let mut points = profile.points.clone();
    points.sort_by_key(|point| point.target_temp_c);
    let lower = points
        .iter()
        .copied()
        .rev()
        .find(|point| point.target_temp_c < target_temp_c)?;
    let upper = points
        .iter()
        .copied()
        .find(|point| point.target_temp_c > target_temp_c)?;
    let ratio = f32::from(target_temp_c - lower.target_temp_c)
        / f32::from(upper.target_temp_c - lower.target_temp_c);
    let lerp = |left: u16, right: u16, upper_bound: u16| {
        (f32::from(left) + ((f32::from(right) - f32::from(left)) * ratio) + 0.5)
            .clamp(0.0, f32::from(upper_bound)) as u16
    };
    let linear_brake_distance = lerp(
        lower.brake_distance_centi_c,
        upper.brake_distance_centi_c,
        5_000,
    );
    let midpoint_weight = 4.0 * ratio * (1.0 - ratio);
    let intermediate_brake_adjustment = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        -0.20
    } else if lower.target_temp_c >= 100 && upper.target_temp_c <= 180 {
        if upper.target_temp_c <= 140 {
            0.55
        } else {
            0.20
        }
    } else {
        0.0
    };
    let interpolated_brake_distance = (f32::from(linear_brake_distance)
        * (1.0 - intermediate_brake_adjustment * midpoint_weight)
        + 0.5) as u16;
    let low_temp_hold_scale = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - (0.20 * midpoint_weight)
    } else {
        1.0
    };
    let low_temp_reheat_scale = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - (0.10 * midpoint_weight)
    } else {
        1.0
    };
    let scale_low_temp_hold =
        |value: u16| (f32::from(value) * low_temp_hold_scale + 0.5).clamp(0.0, 1_000.0) as u16;
    Some(ThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c: interpolated_brake_distance,
        warmup_power_permille: 1_000,
        approach_power_permille: lerp(
            lower.approach_power_permille,
            upper.approach_power_permille,
            1_000,
        ),
        approach_floor_power_permille: lerp(
            lower.approach_floor_power_permille,
            upper.approach_floor_power_permille,
            1_000,
        ),
        approach_damping_exponent_permille: lerp(
            lower.approach_damping_exponent_permille,
            upper.approach_damping_exponent_permille,
            4_000,
        ),
        approach_tail_window_centi_c: lerp(
            lower.approach_tail_window_centi_c,
            upper.approach_tail_window_centi_c,
            5_000,
        ),
        hold_power_permille: scale_low_temp_hold(lerp(
            lower.hold_power_permille,
            upper.hold_power_permille,
            1_000,
        )),
        hold_reheat_power_permille: (f32::from(lerp(
            lower.hold_reheat_power_permille,
            upper.hold_reheat_power_permille,
            1_000,
        )) * low_temp_reheat_scale
            + 0.5) as u16,
        hold_entry_centi_c: lerp(lower.hold_entry_centi_c, upper.hold_entry_centi_c, 5_000),
        hold_exit_centi_c: lerp(lower.hold_exit_centi_c, upper.hold_exit_centi_c, 5_000),
        hold_on_centi_c: lerp(lower.hold_on_centi_c, upper.hold_on_centi_c, 5_000),
        hold_off_centi_c: lerp(lower.hold_off_centi_c, upper.hold_off_centi_c, 5_000),
        overshoot_cutoff_centi_c: lerp(
            lower.overshoot_cutoff_centi_c,
            upper.overshoot_cutoff_centi_c,
            5_000,
        ),
        hold_kp_permille_per_c: lerp(
            lower.hold_kp_permille_per_c,
            upper.hold_kp_permille_per_c,
            10_000,
        ),
        hold_ki_permille_per_c_tick: {
            let interpolated = lerp(
                lower.hold_ki_permille_per_c_tick,
                upper.hold_ki_permille_per_c_tick,
                10_000,
            );
            if interpolated == 0 {
                profile.settings.hold_ki_permille_per_c_tick
            } else {
                interpolated
            }
        },
        hold_blend_ticks: lerp(
            lower.hold_blend_ticks,
            upper.hold_blend_ticks,
            u16::from(u8::MAX),
        )
        .clamp(1, u16::from(u8::MAX)),
        approach_lead_ticks: lerp(
            lower.approach_lead_ticks,
            upper.approach_lead_ticks,
            u16::from(u8::MAX),
        ),
        hold_lead_ticks: lerp(
            lower.hold_lead_ticks,
            upper.hold_lead_ticks,
            u16::from(u8::MAX),
        ),
    })
}

fn thermal_candidate_point_mut(
    profile: &mut ThermalCandidateProfile,
    target_temp_c: i16,
) -> Option<&mut ThermalCandidatePoint> {
    profile
        .points
        .iter_mut()
        .find(|point| point.target_temp_c == target_temp_c)
}

fn thermal_rebuild_profile_from_anchor_targets(
    profile: &mut ThermalCandidateProfile,
    anchor_targets_c: &[i16],
) {
    let mut anchors = anchor_targets_c
        .iter()
        .filter_map(|target_temp_c| thermal_candidate_point(profile, *target_temp_c))
        .collect::<Vec<_>>();
    anchors.sort_by_key(|point| point.target_temp_c);
    if anchors.is_empty() {
        return;
    }
    for point in &mut profile.points {
        if anchors
            .iter()
            .any(|anchor| anchor.target_temp_c == point.target_temp_c)
        {
            continue;
        }
        if point.target_temp_c < anchors[0].target_temp_c
            || point.target_temp_c > anchors[anchors.len() - 1].target_temp_c
        {
            continue;
        }
        let mut lower = anchors[0];
        let mut upper = *anchors.last().unwrap_or(&anchors[0]);
        for anchor in &anchors {
            if anchor.target_temp_c <= point.target_temp_c {
                lower = *anchor;
            }
            if anchor.target_temp_c >= point.target_temp_c {
                upper = *anchor;
                break;
            }
        }
        *point = rebuild_thermal_candidate_point_from_anchor_relations(
            point.target_temp_c,
            lower,
            upper,
        );
    }
    profile.points.sort_by_key(|point| point.target_temp_c);
}

fn rebuild_thermal_candidate_point_from_anchor_relations(
    target_temp_c: i16,
    lower: ThermalCandidatePoint,
    upper: ThermalCandidatePoint,
) -> ThermalCandidatePoint {
    if lower.target_temp_c >= upper.target_temp_c {
        return ThermalCandidatePoint {
            target_temp_c,
            ..lower
        };
    }
    let ratio = (f32::from(target_temp_c - lower.target_temp_c)
        / f32::from(upper.target_temp_c - lower.target_temp_c))
    .clamp(0.0, 1.0);
    let default_point = thermal_default_target_point(target_temp_c);
    let lower_default = thermal_default_target_point(lower.target_temp_c);
    let upper_default = thermal_default_target_point(upper.target_temp_c);

    let scale_power = |target_default: u16,
                       lower_value: u16,
                       lower_default_value: u16,
                       upper_value: u16,
                       upper_default_value: u16| {
        let lower_scale = if lower_default_value == 0 {
            1.0
        } else {
            lower_value as f32 / lower_default_value as f32
        };
        let upper_scale = if upper_default_value == 0 {
            1.0
        } else {
            upper_value as f32 / upper_default_value as f32
        };
        ((target_default as f32) * (lower_scale + ((upper_scale - lower_scale) * ratio)) + 0.5)
            .clamp(0.0, 1_000.0) as u16
    };
    let shift_from_default = |target_default: u16,
                              lower_value: u16,
                              lower_default_value: u16,
                              upper_value: u16,
                              upper_default_value: u16,
                              max: u16| {
        let lower_delta = i32::from(lower_value) - i32::from(lower_default_value);
        let upper_delta = i32::from(upper_value) - i32::from(upper_default_value);
        ((target_default as f32)
            + (lower_delta as f32 + ((upper_delta - lower_delta) as f32 * ratio))
            + 0.5)
            .clamp(0.0, f32::from(max)) as u16
    };

    let hold_power_permille = scale_power(
        default_point.hold_power_permille,
        lower.hold_power_permille,
        lower_default.hold_power_permille,
        upper.hold_power_permille,
        upper_default.hold_power_permille,
    );
    let mut approach_floor_power_permille = scale_power(
        default_point.approach_floor_power_permille,
        lower.approach_floor_power_permille,
        lower_default.approach_floor_power_permille,
        upper.approach_floor_power_permille,
        upper_default.approach_floor_power_permille,
    )
    .max(hold_power_permille);
    let mut approach_power_permille = scale_power(
        default_point.approach_power_permille,
        lower.approach_power_permille,
        lower_default.approach_power_permille,
        upper.approach_power_permille,
        upper_default.approach_power_permille,
    )
    .max(approach_floor_power_permille);
    let warmup_power_permille = 1_000;
    let hold_reheat_power_permille = scale_power(
        default_point.hold_reheat_power_permille,
        lower.hold_reheat_power_permille,
        lower_default.hold_reheat_power_permille,
        upper.hold_reheat_power_permille,
        upper_default.hold_reheat_power_permille,
    )
    .max(hold_power_permille)
    .max(approach_floor_power_permille);

    let default_approach_gap = default_point
        .approach_power_permille
        .saturating_sub(default_point.approach_floor_power_permille);
    approach_power_permille = approach_power_permille
        .max(approach_floor_power_permille.saturating_add((default_approach_gap / 3).max(10)));
    approach_floor_power_permille = approach_floor_power_permille.min(1_000);
    approach_power_permille = approach_power_permille.min(1_000);

    ThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c: shift_from_default(
            default_point.brake_distance_centi_c,
            lower.brake_distance_centi_c,
            lower_default.brake_distance_centi_c,
            upper.brake_distance_centi_c,
            upper_default.brake_distance_centi_c,
            5_000,
        ),
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille: shift_from_default(
            default_point.approach_damping_exponent_permille,
            lower.approach_damping_exponent_permille,
            lower_default.approach_damping_exponent_permille,
            upper.approach_damping_exponent_permille,
            upper_default.approach_damping_exponent_permille,
            4_000,
        ),
        approach_tail_window_centi_c: shift_from_default(
            default_point.approach_tail_window_centi_c,
            lower.approach_tail_window_centi_c,
            lower_default.approach_tail_window_centi_c,
            upper.approach_tail_window_centi_c,
            upper_default.approach_tail_window_centi_c,
            5_000,
        ),
        hold_power_permille,
        hold_reheat_power_permille,
        hold_entry_centi_c: shift_from_default(
            default_point.hold_entry_centi_c,
            lower.hold_entry_centi_c,
            lower_default.hold_entry_centi_c,
            upper.hold_entry_centi_c,
            upper_default.hold_entry_centi_c,
            5_000,
        ),
        hold_exit_centi_c: shift_from_default(
            default_point.hold_exit_centi_c,
            lower.hold_exit_centi_c,
            lower_default.hold_exit_centi_c,
            upper.hold_exit_centi_c,
            upper_default.hold_exit_centi_c,
            5_000,
        ),
        hold_on_centi_c: shift_from_default(
            default_point.hold_on_centi_c,
            lower.hold_on_centi_c,
            lower_default.hold_on_centi_c,
            upper.hold_on_centi_c,
            upper_default.hold_on_centi_c,
            5_000,
        ),
        hold_off_centi_c: shift_from_default(
            default_point.hold_off_centi_c,
            lower.hold_off_centi_c,
            lower_default.hold_off_centi_c,
            upper.hold_off_centi_c,
            upper_default.hold_off_centi_c,
            5_000,
        ),
        overshoot_cutoff_centi_c: shift_from_default(
            default_point.overshoot_cutoff_centi_c,
            lower.overshoot_cutoff_centi_c,
            lower_default.overshoot_cutoff_centi_c,
            upper.overshoot_cutoff_centi_c,
            upper_default.overshoot_cutoff_centi_c,
            5_000,
        ),
        hold_kp_permille_per_c: shift_from_default(
            default_point.hold_kp_permille_per_c,
            lower.hold_kp_permille_per_c,
            lower_default.hold_kp_permille_per_c,
            upper.hold_kp_permille_per_c,
            upper_default.hold_kp_permille_per_c,
            10_000,
        ),
        hold_ki_permille_per_c_tick: shift_from_default(
            default_point.hold_ki_permille_per_c_tick,
            lower.hold_ki_permille_per_c_tick,
            lower_default.hold_ki_permille_per_c_tick,
            upper.hold_ki_permille_per_c_tick,
            upper_default.hold_ki_permille_per_c_tick,
            10_000,
        ),
        hold_blend_ticks: shift_from_default(
            default_point.hold_blend_ticks,
            lower.hold_blend_ticks,
            lower_default.hold_blend_ticks,
            upper.hold_blend_ticks,
            upper_default.hold_blend_ticks,
            u16::from(u8::MAX),
        ),
        approach_lead_ticks: shift_from_default(
            default_point.approach_lead_ticks,
            lower.approach_lead_ticks,
            lower_default.approach_lead_ticks,
            upper.approach_lead_ticks,
            upper_default.approach_lead_ticks,
            u16::from(u8::MAX),
        ),
        hold_lead_ticks: shift_from_default(
            default_point.hold_lead_ticks,
            lower.hold_lead_ticks,
            lower_default.hold_lead_ticks,
            upper.hold_lead_ticks,
            upper_default.hold_lead_ticks,
            u16::from(u8::MAX),
        ),
    }
}

fn tune_thermal_candidate_point(
    previous: ThermalCandidatePoint,
    result: &ThermalStageResult,
) -> ThermalCandidatePoint {
    if !thermal_stage_can_tune(result) {
        return previous;
    }
    let analysis = &result.analysis;
    let settle_limit_ms =
        ThermalFullSpeedStableTracker::settle_limit_ms_for_target(result.target_temp_c);
    let full_speed_failed = result.stop_reason != "completed"
        || result
            .full_speed_to_stable
            .settle_time_ms
            .is_some_and(|value| value > settle_limit_ms)
        || result.full_speed_to_stable.failure_reason.is_some();
    let overshoot_c = result.max_overshoot_c.max(0.0);
    let residual_c = analysis
        .residual_heat_after_hold_entry_c
        .unwrap_or(overshoot_c)
        .max(0.0);
    let under_c = analysis.hold_max_below_target_c.unwrap_or(0.0).max(0.0);
    let over_c = analysis.hold_max_above_target_c.unwrap_or(0.0).max(0.0);
    let hold_p2p_c = result.hold_peak_to_peak_c.max(0.0);
    let equilibrium = analysis
        .hold_median_output_permille
        .unwrap_or(previous.hold_power_permille);
    let hold_p90 = analysis
        .hold_p90_output_permille
        .unwrap_or(previous.hold_reheat_power_permille.max(equilibrium));
    let near_target_power = analysis
        .approach_median_output_permille
        .unwrap_or(previous.approach_floor_power_permille.max(equilibrium));
    let curve_class = analysis.approach_curve_deviation_class;
    let curve_needs_tuning = matches!(
        curve_class,
        Some("brake_late_or_residual" | "underpowered_or_early_coast" | "oscillatory_near_target")
    );
    let curve_overshoot = curve_class == Some("brake_late_or_residual");
    let curve_underpowered = curve_class == Some("underpowered_or_early_coast");
    let curve_oscillation = curve_class == Some("oscillatory_near_target");
    let entering_below_target = analysis.first_hold_error_c.unwrap_or(0.0).max(0.0);
    let entering_above_target = (-analysis.first_hold_error_c.unwrap_or(0.0)).max(0.0);
    let hold_gate_lag = full_speed_failed
        && analysis.hold_sample_count == 0
        && result.guard.hold_threshold_crossed_at_ms.is_some()
        && !curve_underpowered;
    let approach_only_underpowered =
        (full_speed_failed && analysis.hold_sample_count == 0 && !hold_gate_lag)
            || curve_underpowered;
    let high_temp_power_limited = approach_only_underpowered
        && result.target_temp_c >= 180
        && near_target_power >= 900
        && analysis
            .approach_median_slope_c_per_s
            .is_some_and(|slope_c_per_s| slope_c_per_s <= 1.0);
    let starved_low_temp_hold = full_speed_failed
        && result.target_temp_c <= 120
        && analysis.hold_sample_count > 0
        && hold_p90 == 0
        && analysis
            .hold_mean_error_c
            .is_some_and(|mean_error_c| mean_error_c > 0.2)
        && under_c > over_c + 0.4;
    let stability_overshoot = full_speed_failed
        && analysis.hold_sample_count > 0
        && over_c > ThermalFullSpeedStableTracker::STABLE_BAND_C
        && (result.target_temp_c <= 120 || over_c >= under_c)
        && !starved_low_temp_hold;
    let entry_residual_dominant = analysis.first_hold_error_c.is_some()
        && residual_c >= 1.5
        && over_c >= ThermalFullSpeedStableTracker::STABLE_BAND_C
        && over_c >= under_c + 0.4;
    let overshoot_dominant = curve_overshoot
        || overshoot_c > 3.0
        || stability_overshoot
        || entry_residual_dominant
        || (residual_c > 2.5 && entering_above_target > 0.5 && over_c > under_c);
    let hold_ripple = curve_oscillation || (analysis.hold_sample_count > 0 && hold_p2p_c > 3.0);
    let timely_hold_but_late_stability = full_speed_failed
        && result
            .guard
            .first_hold_at_ms
            .zip(result.full_speed_to_stable.warmup_exited_at_ms)
            .is_some_and(|(first_hold_at_ms, warmup_exited_at_ms)| {
                first_hold_at_ms.saturating_sub(warmup_exited_at_ms) <= settle_limit_ms
            })
        && result
            .full_speed_to_stable
            .stable_window_started_at_ms
            .zip(result.full_speed_to_stable.warmup_exited_at_ms)
            .is_some_and(|(stable_started_at_ms, warmup_exited_at_ms)| {
                stable_started_at_ms.saturating_sub(warmup_exited_at_ms) > settle_limit_ms
            })
        && under_c <= ThermalFullSpeedStableTracker::STABLE_BAND_C
        && over_c <= ThermalFullSpeedStableTracker::STABLE_BAND_C;
    let low_temp_hold_entry_carry = full_speed_failed
        && result.target_temp_c <= 120
        && analysis.hold_sample_count > 0
        && analysis
            .hold_p90_output_permille
            .is_some_and(|output| output > previous.hold_power_permille)
        && hold_p2p_c > ThermalFullSpeedStableTracker::STABLE_BAND_C + 0.5
        && entering_below_target >= 0.4
        && residual_c >= ThermalFullSpeedStableTracker::STABLE_BAND_C
        && over_c > ThermalFullSpeedStableTracker::STABLE_BAND_C;
    let underpowered = curve_underpowered
        || starved_low_temp_hold
        || (full_speed_failed && !overshoot_dominant && !hold_ripple)
        || (!hold_ripple
            && !overshoot_dominant
            && (under_c > over_c + 0.4 || entering_below_target > 0.4));
    let bursty_low_temp_hold = result.target_temp_c <= 120
        && analysis.hold_median_output_permille == Some(0)
        && hold_p90 >= previous.hold_power_permille.saturating_add(20)
        && under_c > over_c + 0.4
        && analysis
            .hold_mean_error_c
            .is_some_and(|mean_error_c| mean_error_c > 0.2);
    let mut tuned = previous;

    if !full_speed_failed && overshoot_c <= 3.0 && hold_p2p_c <= 3.0 && !curve_needs_tuning {
        return previous;
    }

    if high_temp_power_limited {
        let stable_entry_centi_c =
            (ThermalFullSpeedStableTracker::STABLE_BAND_C * 100.0).round() as u16;
        tuned.hold_entry_centi_c = stable_entry_centi_c;
        tuned.hold_exit_centi_c = tuned
            .hold_exit_centi_c
            .max(stable_entry_centi_c.saturating_add(10));
        tuned.brake_distance_centi_c = tuned
            .hold_entry_centi_c
            .saturating_add(10)
            .clamp(100, 5_000);
        tuned.warmup_power_permille = 1_000;
        tuned.approach_power_permille = 1_000;
        tuned.approach_floor_power_permille = 1_000;
        tuned.approach_damping_exponent_permille = 100;
        tuned.approach_lead_ticks = 0;
        tuned.hold_power_permille = near_target_power.saturating_add(50).clamp(950, 1_000);
        tuned.hold_reheat_power_permille = 1_000;
        tuned.hold_off_centi_c = tuned.hold_off_centi_c.min(80);
        tuned.overshoot_cutoff_centi_c = tuned
            .overshoot_cutoff_centi_c
            .min(180)
            .max(tuned.hold_off_centi_c.saturating_add(40));
    } else if timely_hold_but_late_stability {
        let hold_exit_target = ((under_c + 0.2) * 100.0).round().clamp(80.0, 300.0) as u16;
        tuned.hold_exit_centi_c = tuned.hold_exit_centi_c.max(hold_exit_target);
    } else if low_temp_hold_entry_carry {
        let previous_hold_lead_ticks = tuned.hold_lead_ticks;
        tuned.hold_lead_ticks = tuned
            .hold_lead_ticks
            .saturating_add(if residual_c >= 2.0 { 2 } else { 1 })
            .min(8);
        if tuned.hold_lead_ticks == previous_hold_lead_ticks {
            tuned.hold_power_permille = tuned.hold_power_permille.saturating_sub(20).max(40);
            tuned.hold_off_centi_c = tuned.hold_off_centi_c.saturating_add(20).min(400);
        }
        tuned.hold_reheat_power_permille = tuned
            .hold_reheat_power_permille
            .saturating_sub(30)
            .max(tuned.hold_power_permille);
    } else if hold_gate_lag {
        let lead_step = if result.target_temp_c >= 120 { 1 } else { 2 };
        tuned.approach_lead_ticks = tuned.approach_lead_ticks.saturating_add(lead_step).min(12);
    } else if approach_only_underpowered {
        let power_step = if result.target_temp_c >= 180 { 120 } else { 80 };
        let lead_step = (tuned.approach_lead_ticks / 2).max(2);
        tuned.approach_floor_power_permille = step_toward_u16(
            tuned.approach_floor_power_permille,
            near_target_power
                .saturating_add(power_step)
                .max(tuned.approach_floor_power_permille),
            power_step,
            tuned.hold_power_permille,
            1_000,
        );
        tuned.approach_power_permille = tuned
            .approach_power_permille
            .max(tuned.approach_floor_power_permille.saturating_add(80))
            .min(1_000);
        tuned.approach_damping_exponent_permille = tuned
            .approach_damping_exponent_permille
            .saturating_sub(90)
            .clamp(100, 4_000);
        tuned.approach_lead_ticks = tuned.approach_lead_ticks.saturating_sub(lead_step);
        tuned.brake_distance_centi_c = tuned
            .brake_distance_centi_c
            .saturating_sub(if result.target_temp_c <= 120 { 120 } else { 60 })
            .max(100);
    } else if underpowered {
        let power_step = if result.target_temp_c >= 180 {
            160
        } else {
            120
        };
        let sustain_target = near_target_power
            .max(equilibrium.saturating_add(power_step))
            .max(
                previous
                    .hold_reheat_power_permille
                    .saturating_add(power_step / 2),
            );
        tuned.hold_power_permille = step_toward_u16(
            tuned.hold_power_permille,
            equilibrium
                .max(previous.hold_power_permille)
                .saturating_add(40),
            100,
            0,
            1_000,
        );
        tuned.approach_floor_power_permille = step_toward_u16(
            tuned.approach_floor_power_permille,
            sustain_target,
            power_step,
            tuned.hold_power_permille,
            1_000,
        );
        tuned.hold_reheat_power_permille = step_toward_u16(
            tuned.hold_reheat_power_permille,
            sustain_target.saturating_add(40),
            power_step,
            tuned.approach_floor_power_permille,
            1_000,
        );
        tuned.approach_power_permille = tuned
            .approach_power_permille
            .max(tuned.hold_reheat_power_permille.saturating_add(80))
            .min(1_000);
        tuned.warmup_power_permille = tuned
            .warmup_power_permille
            .max(tuned.approach_power_permille.saturating_add(100))
            .min(1_000);
        tuned.approach_damping_exponent_permille = tuned
            .approach_damping_exponent_permille
            .saturating_sub(180)
            .clamp(100, 4_000);
        tuned.brake_distance_centi_c = tuned.brake_distance_centi_c.saturating_sub(120).max(100);
        tuned.hold_kp_permille_per_c = tuned
            .hold_kp_permille_per_c
            .saturating_add(4)
            .clamp(8, 10_000);
    } else if overshoot_dominant {
        let bounded_low_temp_entry_residual = result.target_temp_c <= 120
            && stability_overshoot
            && residual_c <= 3.5
            && analysis.hold_sample_count > 0
            && result
                .guard
                .first_hold_at_ms
                .zip(result.full_speed_to_stable.warmup_exited_at_ms)
                .is_some_and(|(first_hold_at_ms, warmup_exited_at_ms)| {
                    first_hold_at_ms.saturating_sub(warmup_exited_at_ms) <= settle_limit_ms
                });
        let coast_gate_limited = stability_overshoot
            && result.target_temp_c <= 120
            && residual_c <= 3.5
            && analysis.hold_sample_count > 0;
        let brake_step = if bounded_low_temp_entry_residual {
            80
        } else if coast_gate_limited {
            0
        } else {
            ((residual_c * 100.0) + (overshoot_c * 70.0))
                .round()
                .clamp(if stability_overshoot { 100.0 } else { 80.0 }, 350.0) as u16
        };
        tuned.brake_distance_centi_c = tuned
            .brake_distance_centi_c
            .saturating_add(brake_step)
            .clamp(100, 5_000);
        tuned.approach_damping_exponent_permille = tuned
            .approach_damping_exponent_permille
            .saturating_add(if bounded_low_temp_entry_residual {
                50
            } else if residual_c > 3.0 {
                200
            } else {
                100
            })
            .clamp(100, 4_000);
        if bounded_low_temp_entry_residual {
            let hold_delay_ms = result
                .guard
                .first_hold_at_ms
                .zip(result.full_speed_to_stable.warmup_exited_at_ms)
                .map(|(first_hold_at_ms, warmup_exited_at_ms)| {
                    first_hold_at_ms.saturating_sub(warmup_exited_at_ms)
                })
                .unwrap_or_default();
            if hold_delay_ms >= 9_000 {
                tuned.hold_entry_centi_c = tuned.hold_entry_centi_c.saturating_add(40).min(250);
            } else {
                let cutoff_step = ((over_c - ThermalFullSpeedStableTracker::STABLE_BAND_C) * 100.0)
                    .round()
                    .clamp(20.0, 60.0) as u16;
                tuned.overshoot_cutoff_centi_c = tuned
                    .overshoot_cutoff_centi_c
                    .saturating_sub(cutoff_step)
                    .max(50);
                tuned.hold_off_centi_c = tuned
                    .hold_off_centi_c
                    .min(tuned.overshoot_cutoff_centi_c.saturating_sub(40).max(50));
            }
        } else if tuned.approach_lead_ticks < 12 {
            tuned.approach_lead_ticks = tuned
                .approach_lead_ticks
                .saturating_add(if result.target_temp_c <= 120 && residual_c > 3.0 {
                    2
                } else {
                    1
                })
                .min(12);
        }
        if coast_gate_limited && !entry_residual_dominant && !bounded_low_temp_entry_residual {
            let coast_step = ((residual_c - 0.8).max(overshoot_c - 1.5) * 100.0)
                .round()
                .clamp(50.0, 250.0) as u16;
            tuned.hold_exit_centi_c = tuned
                .hold_exit_centi_c
                .saturating_add(coast_step)
                .min(tuned.brake_distance_centi_c.saturating_sub(10))
                .clamp(tuned.hold_entry_centi_c, 5_000);
        }
        tuned.approach_floor_power_permille = tuned
            .approach_floor_power_permille
            .max(equilibrium.saturating_sub(30));
        let equilibrium_observation_ready = !full_speed_failed || analysis.hold_sample_count >= 120;
        let equilibrium_was_observed = equilibrium_observation_ready
            && (analysis
                .hold_max_below_target_c
                .is_some_and(|below_target_c| below_target_c >= 0.3)
                || analysis
                    .hold_mean_error_c
                    .is_some_and(|mean_error_c| mean_error_c.abs() <= 1.0));
        let coasted_through_hold = equilibrium_observation_ready
            && analysis
                .hold_max_below_target_c
                .is_some_and(|below_target_c| below_target_c <= 0.1)
            && analysis
                .hold_mean_error_c
                .is_some_and(|mean_error_c| mean_error_c < -1.5)
            && analysis
                .hold_median_output_permille
                .is_some_and(|output| output <= previous.hold_power_permille);
        if coasted_through_hold {
            let floor_step = if residual_c > 5.0 { 100 } else { 60 };
            tuned.approach_floor_power_permille = tuned
                .approach_floor_power_permille
                .saturating_sub(floor_step)
                .max(tuned.hold_power_permille);
            tuned.hold_reheat_power_permille = tuned
                .hold_reheat_power_permille
                .saturating_sub(floor_step)
                .max(tuned.hold_power_permille);
        }
        if equilibrium_was_observed {
            tuned.hold_reheat_power_permille = tuned
                .hold_reheat_power_permille
                .saturating_sub(100)
                .max(tuned.hold_power_permille);
            tuned.hold_power_permille = step_toward_u16(
                tuned.hold_power_permille,
                equilibrium.saturating_sub(30),
                80,
                0,
                1_000,
            );
        }
        if result.target_temp_c <= 120
            && residual_c > 5.0
            && analysis
                .approach_median_slope_c_per_s
                .is_some_and(|slope_c_per_s| slope_c_per_s > 2.0)
        {
            tuned.warmup_power_permille = tuned
                .warmup_power_permille
                .saturating_sub(250)
                .max(tuned.approach_power_permille);
        }
        tuned.hold_blend_ticks = tuned.hold_blend_ticks.saturating_sub(3).max(1);
        tuned.hold_kp_permille_per_c = tuned.hold_kp_permille_per_c.saturating_sub(4).max(8);
    } else if hold_ripple {
        let hold_ripple_equilibrium = if bursty_low_temp_hold {
            previous
                .hold_power_permille
                .saturating_add(30)
                .max(hold_p90.saturating_sub(100))
        } else {
            equilibrium
        };
        let high_temp_entry_carry = result.target_temp_c >= 180
            && entering_below_target >= 0.2
            && residual_c >= 1.6
            && over_c >= 1.0;
        let saturated_high_temp_hold = result.target_temp_c >= 180
            && equilibrium >= 950
            && hold_p90 >= 990
            && over_c >= 1.0
            && under_c >= 1.0;
        let reheat_gap = hold_p90.saturating_sub(if bursty_low_temp_hold {
            hold_ripple_equilibrium
        } else {
            equilibrium
        });
        let bounded_reheat_gap = if over_c >= under_c {
            (reheat_gap / 2).clamp(40, 100)
        } else {
            reheat_gap.clamp(80, 160)
        };
        tuned.hold_power_permille = step_toward_u16(
            tuned.hold_power_permille,
            hold_ripple_equilibrium,
            80,
            0,
            1_000,
        );
        let approach_floor_target = if under_c > over_c {
            previous
                .approach_floor_power_permille
                .max(tuned.hold_power_permille.saturating_add(20))
        } else {
            tuned.hold_power_permille.saturating_add(20)
        };
        tuned.approach_floor_power_permille = tuned
            .approach_floor_power_permille
            .max(approach_floor_target)
            .min(1_000);
        tuned.hold_reheat_power_permille = tuned
            .hold_power_permille
            .saturating_add(bounded_reheat_gap)
            .max(tuned.approach_floor_power_permille.saturating_add(40))
            .min(1_000);
        if under_c > 3.0 {
            let hold_exit_target = (under_c * 100.0 * 0.67).round().clamp(100.0, 300.0) as u16;
            tuned.hold_exit_centi_c = tuned.hold_exit_centi_c.max(hold_exit_target);
            if result.target_temp_c >= 180 {
                tuned.hold_lead_ticks = tuned.hold_lead_ticks.saturating_add(1).min(8);
            }
        }
        if saturated_high_temp_hold {
            let hold_off_c = f64::from(tuned.hold_off_centi_c) / 100.0;
            let cutoff_target_c = ((over_c - (0.7 * hold_off_c)) / 0.3)
                .max(hold_off_c + 0.4)
                .clamp(1.2, 6.0);
            tuned.overshoot_cutoff_centi_c = tuned
                .overshoot_cutoff_centi_c
                .max((cutoff_target_c * 100.0).round() as u16);
            tuned.hold_blend_ticks = tuned.hold_blend_ticks.min(4).max(1);
        } else if high_temp_entry_carry {
            tuned.hold_on_centi_c = step_toward_u16(tuned.hold_on_centi_c, 140, 60, 20, 250);
            tuned.hold_off_centi_c = tuned.hold_off_centi_c.saturating_sub(30).max(40);
            tuned.hold_blend_ticks = tuned.hold_blend_ticks.saturating_sub(4).max(1);
            tuned.hold_kp_permille_per_c = tuned
                .hold_kp_permille_per_c
                .saturating_add(6)
                .clamp(8, 10_000);
        } else if entering_above_target > 0.2 || over_c > under_c {
            tuned.hold_on_centi_c = tuned.hold_on_centi_c.saturating_add(30).clamp(20, 250);
            tuned.hold_exit_centi_c = tuned
                .hold_exit_centi_c
                .max(tuned.hold_on_centi_c)
                .clamp(20, 500);
            tuned.hold_blend_ticks = tuned.hold_blend_ticks.saturating_sub(3).max(1);
            tuned.hold_kp_permille_per_c = tuned.hold_kp_permille_per_c.saturating_sub(3).max(8);
        } else {
            tuned.hold_kp_permille_per_c = tuned
                .hold_kp_permille_per_c
                .saturating_add(3)
                .clamp(8, 10_000);
        }
    } else {
        return previous;
    }

    tuned.hold_reheat_power_permille = tuned
        .hold_reheat_power_permille
        .max(tuned.hold_power_permille);
    tuned.approach_power_permille = tuned
        .approach_power_permille
        .max(tuned.approach_floor_power_permille)
        .min(1_000);
    tuned.warmup_power_permille = tuned
        .warmup_power_permille
        .max(tuned.approach_power_permille)
        .min(1_000);
    tuned
}

fn step_toward_u16(current: u16, target: u16, max_step: u16, min: u16, max: u16) -> u16 {
    let (min, max) = if min <= max { (min, max) } else { (max, min) };
    let bounded_target = target.clamp(min, max);
    if bounded_target > current {
        current
            .saturating_add((bounded_target - current).min(max_step))
            .clamp(min, max)
    } else {
        current
            .saturating_sub((current - bounded_target).min(max_step))
            .clamp(min, max)
    }
}

fn percentile_u16(values: &[u16], percentile: f64) -> Option<u16> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() - 1) as f64 * percentile.clamp(0.0, 1.0) + 0.5) as usize;
    sorted.get(index.min(sorted.len() - 1)).copied()
}

fn percentile_f64(values: &[f64], percentile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((sorted.len() - 1) as f64 * percentile.clamp(0.0, 1.0) + 0.5) as usize;
    sorted.get(index.min(sorted.len() - 1)).copied()
}

fn thermal_heater_parameters_value(
    target_temp_c: i16,
    thermal_profile: Option<&Value>,
    mode: &'static str,
) -> Value {
    let interpolated_profile = thermal_profile
        .cloned()
        .map(thermal_candidate_profile_from_value);
    let effective_settings = interpolated_profile
        .as_ref()
        .map(|profile| profile.settings.clone())
        .unwrap_or_else(thermal_default_settings);
    let interpolated_point = interpolated_profile
        .as_ref()
        .and_then(|profile| thermal_interpolated_candidate_point(profile, target_temp_c))
        .map(|point| thermal_effective_candidate_point(point, &effective_settings));
    let point_value = interpolated_point.map(|point| {
        thermal_candidate_profile_to_value(&ThermalCandidateProfile {
            settings: effective_settings.clone(),
            points: vec![point],
        })["points"][0]
            .clone()
    });
    let point = point_value.as_ref();
    let approach_tail_window_centi_c = point
        .and_then(|point| point.get("approachTailWindowCentiC"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u16;
    let (
        brake_distance_centi_c,
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        hold_power_permille,
        hold_reheat_power_permille,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks,
    ) = if let Some(point) = point {
        (
            point
                .get("brakeDistanceCentiC")
                .and_then(Value::as_u64)
                .unwrap_or(750) as u16,
            1_000,
            point
                .get("approachPowerPermille")
                .and_then(Value::as_u64)
                .unwrap_or(320) as u16,
            point
                .get("approachFloorPowerPermille")
                .and_then(Value::as_u64)
                .unwrap_or(220) as u16,
            point
                .get("approachDampingExponentPermille")
                .and_then(Value::as_u64)
                .unwrap_or(1_000) as u16,
            point
                .get("holdPowerPermille")
                .and_then(Value::as_u64)
                .unwrap_or(220) as u16,
            point
                .get("holdReheatPowerPermille")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
            point
                .get("holdEntryCentiC")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
            point
                .get("holdExitCentiC")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
            point
                .get("holdOnCentiC")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
            point
                .get("holdOffCentiC")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
            point
                .get("overshootCutoffCentiC")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
            point
                .get("holdKpPermillePerC")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
            point
                .get("holdKiPermillePerCTick")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
            point
                .get("holdBlendTicks")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
            point
                .get("approachLeadTicks")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
            point
                .get("holdLeadTicks")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u16,
        )
    } else {
        thermal_default_target_values(target_temp_c)
    };
    let settings = thermal_profile
        .and_then(|profile| profile.get("settings"))
        .cloned()
        .unwrap_or_else(thermal_default_settings_value);
    json!({
        "mode": mode,
        "targetTempC": target_temp_c,
        "warmupPowerPermille": warmup_power_permille,
        "brakeDistanceCentiC": brake_distance_centi_c,
        "approachPowerPermille": approach_power_permille,
        "approachFloorPowerPermille": approach_floor_power_permille,
        "approachDampingExponentPermille": approach_damping_exponent_permille,
        "approachTailWindowCentiC": approach_tail_window_centi_c,
        "holdPowerPermille": hold_power_permille,
        "holdReheatPowerPermille": hold_reheat_power_permille,
        "holdEntryCentiC": hold_entry_centi_c,
        "holdExitCentiC": hold_exit_centi_c,
        "holdOnCentiC": hold_on_centi_c,
        "holdOffCentiC": hold_off_centi_c,
        "overshootCutoffCentiC": overshoot_cutoff_centi_c,
        "holdKpPermillePerC": hold_kp_permille_per_c,
        "holdKiPermillePerCTick": hold_ki_permille_per_c_tick,
        "holdBlendTicks": hold_blend_ticks,
        "approachLeadTicks": approach_lead_ticks,
        "holdLeadTicks": hold_lead_ticks,
        "settings": settings,
    })
}

fn thermal_effective_candidate_point(
    mut point: ThermalCandidatePoint,
    settings: &ThermalCandidateSettings,
) -> ThermalCandidatePoint {
    if point.hold_entry_centi_c == 0 {
        point.hold_entry_centi_c = settings.hold_entry_centi_c;
    }
    if point.hold_exit_centi_c == 0 {
        point.hold_exit_centi_c = settings.hold_exit_centi_c;
    }
    if point.hold_on_centi_c == 0 {
        point.hold_on_centi_c = settings.hold_on_centi_c;
    }
    if point.hold_off_centi_c == 0 {
        point.hold_off_centi_c = settings.hold_off_centi_c;
    }
    if point.overshoot_cutoff_centi_c == 0 {
        point.overshoot_cutoff_centi_c = settings.overshoot_cutoff_centi_c;
    }
    if point.hold_kp_permille_per_c == 0 {
        point.hold_kp_permille_per_c = settings.hold_kp_permille_per_c;
    }
    if point.hold_ki_permille_per_c_tick == 0 {
        point.hold_ki_permille_per_c_tick = settings.hold_ki_permille_per_c_tick;
    }
    if point.hold_blend_ticks == 0 {
        point.hold_blend_ticks = settings.hold_blend_ticks;
    }
    if point.hold_reheat_power_permille == 0 {
        point.hold_reheat_power_permille = settings.hold_reheat_power_permille;
    }
    if point.hold_reheat_power_permille == 0 {
        point.hold_reheat_power_permille = point.hold_power_permille;
    }
    point.warmup_power_permille = 1_000;
    point
}

impl BenchSourceLiveTelemetry {
    fn to_value(&self) -> Value {
        json!({
            "voltageMv": self.voltage_mv,
            "currentMa": self.current_ma,
            "powerMw": self.power_mw,
            "sampleUptimeMs": self.sample_uptime_ms,
            "status": self.status,
        })
    }
}

fn heater_telemetry_value(
    status: &Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let heater_output_percent = require_status_u64(status, "heaterOutputPercent")?;
    Ok(json!({
        "currentTempC": require_status_f64(status, "currentTempC")?,
        "hotplateVoltageMv": require_status_u64(status, "voltageMv")?,
        "ppsRequestMv": require_status_u16(status, "pdRequestMv")?,
        "ppsContractMv": require_status_u16(status, "pdContractMv")?,
        "heaterEnabled": status
            .get("heaterEnabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: heaterEnabled"))?,
        "heaterOutputPercent": heater_output_percent,
        "heaterPhysicalOutputPercent": status
            .get("heaterPhysicalOutputPercent")
            .and_then(Value::as_u64)
            .unwrap_or(heater_output_percent),
        "heaterControlIntervalMs": status
            .get("heaterControlIntervalMs")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        "heaterControlCycleMs": status
            .get("heaterControlCycleMs")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        "heaterErrorC": status
            .get("heaterErrorC")
            .and_then(Value::as_f64),
        "heaterControlErrorC": status
            .get("heaterControlErrorC")
            .and_then(Value::as_f64),
        "heaterFilteredTempC": status
            .get("heaterFilteredTempC")
            .and_then(Value::as_f64),
        "heaterFilteredSlopeCPerS": status
            .get("heaterFilteredSlopeCPerS")
            .and_then(Value::as_f64),
        "heaterCoastActive": status
            .get("heaterCoastActive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "thermalControl": status
            .get("thermalControl")
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: thermalControl"))?,
    }))
}

fn write_dry_thermal_ladder(
    samples_writer: &mut BufWriter<File>,
    run_id: &str,
    test_phase: &str,
    source_voltage_mv: u16,
    source_current_ma: u16,
    thermal_profile: Option<&Value>,
    heater_parameter_mode: &'static str,
    target_temps_c: &[i16],
    sample_index: &mut usize,
) -> Result<Vec<ThermalStageResult>, Box<dyn std::error::Error + Send + Sync>> {
    let mut results = Vec::new();
    for &target_temp_c in target_temps_c {
        let rise_time_ms = u64::from(target_temp_c as u16) * 980;
        let max_overshoot_c = 1.8;
        let hold_peak_to_peak_c = 1.6;
        let heater_parameters =
            thermal_heater_parameters_value(target_temp_c, thermal_profile, heater_parameter_mode);
        let mut synthetic_stage_samples = Vec::new();
        for phase in ["warmup", "hold"] {
            let heater_output_percent = if phase == "warmup" { 100 } else { 26 };
            let temp_c = if phase == "warmup" {
                f64::from(target_temp_c) - 0.5
            } else {
                f64::from(target_temp_c) + max_overshoot_c / 2.0
            };
            let elapsed_ms = if phase == "warmup" {
                rise_time_ms.saturating_sub(1_000)
            } else {
                rise_time_ms
            };
            let sample = json!({
                "runId": run_id,
                "sampleIndex": *sample_index,
                "capturedAtUnixMs": current_unix_millis(),
                "elapsedMs": elapsed_ms,
                "testPhase": test_phase,
                "phase": phase,
                "targetTempC": target_temp_c,
                "source": {
                    "mode": "dry_run",
                    "requestedVoltageMv": source_voltage_mv,
                    "requestedCurrentLimitMa": source_current_ma,
                },
                "sourceTelemetry": {
                    "voltageMv": source_voltage_mv,
                    "currentMa": 0,
                    "powerMw": 0,
                    "sampleUptimeMs": 0,
                    "status": "dry_run",
                },
                "heaterTelemetry": {
                    "currentTempC": temp_c,
                    "hotplateVoltageMv": source_voltage_mv,
                    "ppsRequestMv": source_voltage_mv,
                    "ppsContractMv": source_voltage_mv,
                    "heaterEnabled": true,
                    "heaterOutputPercent": heater_output_percent,
                    "heaterPhysicalOutputPercent": heater_output_percent,
                },
                "heaterParameters": heater_parameters,
                "status": synthetic_thermal_status(
                    target_temp_c,
                    temp_c,
                    source_voltage_mv,
                    source_current_ma,
                    heater_output_percent,
                ),
            });
            writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
            synthetic_stage_samples.push(ThermalReplayStageSample {
                elapsed_ms,
                current_temp_c: temp_c,
                heater_output_percent,
                control_phase: Some(if phase == "warmup" {
                    "approach".to_string()
                } else {
                    "hold".to_string()
                }),
                control_phase_in_hold: phase == "hold",
                source_voltage_mv: Some(u64::from(source_voltage_mv)),
                source_current_ma: Some(0),
                source_power_mw: Some(0),
            });
            *sample_index = sample_index.saturating_add(1);
        }
        let mut analysis = thermal_replay_stage_analysis(&synthetic_stage_samples, target_temp_c);
        let guard = ThermalApproachGuardAnalysis {
            hold_threshold_temp_c: f64::from(target_temp_c) - 0.5,
            approach_started_at_ms: Some(rise_time_ms.saturating_sub(1_000)),
            hold_threshold_crossed_at_ms: Some(rise_time_ms),
            first_hold_at_ms: Some(rise_time_ms),
            warmup_reentered_at_ms: None,
        };
        let full_speed_to_stable = ThermalFullSpeedStableAnalysis {
            warmup_exited_at_ms: Some(rise_time_ms.saturating_sub(7_500)),
            stable_window_started_at_ms: Some(rise_time_ms),
            stable_window_verified_at_ms: Some(rise_time_ms),
            settle_time_ms: Some(7_500),
            failure_reason: None,
        };
        let curve_samples = thermal_stage_approach_curve_sample_series(
            &synthetic_stage_samples,
            target_temp_c,
            &guard,
            &analysis,
        );
        if analysis.approach_curve_max_above_c.is_none()
            || analysis.approach_curve_max_below_c.is_none()
        {
            let mut max_above_c = 0.0f64;
            let mut max_below_c = 0.0f64;
            for (sample, reference_temp_c) in
                synthetic_stage_samples.iter().zip(curve_samples.iter())
            {
                if let Some(reference_temp_c) = reference_temp_c {
                    let deviation_c = sample.current_temp_c - reference_temp_c;
                    max_above_c = max_above_c.max(deviation_c.max(0.0));
                    max_below_c = max_below_c.max((-deviation_c).max(0.0));
                }
            }
            analysis.approach_curve_max_above_c = Some(max_above_c);
            analysis.approach_curve_max_below_c = Some(max_below_c);
        }
        results.push(ThermalStageResult {
            target_temp_c,
            rise_time_ms,
            max_overshoot_c,
            hold_peak_to_peak_c,
            sample_count: 2,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: {
                analysis.approach_median_output_permille = heater_parameters
                    .get("approachFloorPowerPermille")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16);
                analysis.approach_median_slope_c_per_s = Some(0.35);
                analysis.hold_median_output_permille = heater_parameters
                    .get("holdPowerPermille")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16);
                analysis.hold_p90_output_permille = heater_parameters
                    .get("holdReheatPowerPermille")
                    .and_then(Value::as_u64)
                    .map(|value| value as u16);
                analysis.hold_mean_error_c = Some(0.0);
                analysis.hold_max_above_target_c = Some(max_overshoot_c / 2.0);
                analysis.hold_max_below_target_c = Some(hold_peak_to_peak_c / 2.0);
                analysis
            },
            guard,
            full_speed_to_stable,
        });
    }
    Ok(results)
}

fn synthetic_thermal_status(
    target_temp_c: i16,
    current_temp_c: f64,
    source_voltage_mv: u16,
    source_current_ma: u16,
    heater_output_percent: u8,
) -> Value {
    json!({
        "mode": "sampling",
        "heaterEnabled": true,
        "heaterOutputPercent": heater_output_percent,
        "heaterPhysicalOutputPercent": heater_output_percent,
        "currentTempC": current_temp_c,
        "targetTempC": target_temp_c,
        "voltageMv": source_voltage_mv,
        "currentMa": source_current_ma,
        "boardTempCenti": (current_temp_c * 100.0) as i32,
        "rtdRawAdcMv": 1000,
        "vinRawAdcMv": 1000,
        "pdRequestMv": source_voltage_mv,
        "pdContractMv": source_voltage_mv,
        "pdState": "ready",
        "manualPpsEnabled": false,
        "fanEnabled": true,
        "fanPwmPermille": 500,
    })
}

async fn arm_thermal_self_test_target(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    target_temp_c: i16,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    arm_thermal_self_test_heater(client, resolved, lease_id, true, target_temp_c).await
}

async fn preview_and_prepare_thermal_self_test_target(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    profile_mode: ThermalProfileMode,
    profile: &Value,
    target_temp_c: i16,
    heater_parameters: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let preview_status = request_leased(
        client,
        resolved,
        lease_id,
        Method::PUT,
        "/runtime",
        Some(thermal_profile_preview_runtime_body(
            profile_mode,
            profile.clone(),
        )),
    )
    .await?;
    verify_thermal_profile_mode_readback(&preview_status, profile_mode)?;
    if !require_status_bool(&preview_status, "thermalControlProfilePreview")? {
        return Err("thermal profile preview did not enable preview mode".into());
    }

    let status =
        arm_thermal_self_test_heater(client, resolved, lease_id, false, target_temp_c).await?;
    verify_thermal_control_readback(&status, heater_parameters, "preview")
}

fn verify_thermal_profile_mode_readback(
    status: &Value,
    expected_mode: ThermalProfileMode,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let expected_mode_value = expected_mode.as_str();
    let expected_bank = expected_thermal_profile_mode_bank(status, expected_mode)?;
    if status.get("thermalProfileMode").and_then(Value::as_str) != Some(expected_mode_value) {
        return Err(format!(
            "thermal profile mode readback mismatch: expected {expected_mode_value}, got {}",
            status
                .get("thermalProfileMode")
                .and_then(Value::as_str)
                .unwrap_or("missing")
        )
        .into());
    }
    if status
        .get("thermalProfileResolvedBank")
        .and_then(Value::as_str)
        != Some(expected_bank)
    {
        return Err(format!(
            "thermal profile bank readback mismatch: expected {expected_bank}, got {}",
            status
                .get("thermalProfileResolvedBank")
                .and_then(Value::as_str)
                .unwrap_or("missing")
        )
        .into());
    }
    Ok(())
}

fn verify_thermal_control_readback(
    status: &Value,
    expected: &Value,
    expected_source: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !require_status_bool(status, "thermalControlProfilePreview")? {
        return Err("thermal profile preview was cleared before heater arm".into());
    }
    let actual = status
        .get("thermalControl")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "status missing object: thermalControl",
            )
        })?;
    if actual.get("profileActive").and_then(Value::as_bool) != Some(true) {
        return Err("thermal control readback reports no active profile".into());
    }
    if actual.get("profileCoversTarget").and_then(Value::as_bool) != Some(true) {
        return Err("thermal control profile does not cover the requested target".into());
    }
    if actual.get("profileSource").and_then(Value::as_str) != Some(expected_source) {
        return Err(format!(
            "thermal control profile source mismatch: expected {expected_source}, got {}",
            actual
                .get("profileSource")
                .and_then(Value::as_str)
                .unwrap_or("missing")
        )
        .into());
    }

    const POINT_FIELDS: &[&str] = &[
        "targetTempC",
        "brakeDistanceCentiC",
        "warmupPowerPermille",
        "approachPowerPermille",
        "approachFloorPowerPermille",
        "approachDampingExponentPermille",
        "approachTailWindowCentiC",
        "holdPowerPermille",
        "holdReheatPowerPermille",
        "holdEntryCentiC",
        "holdExitCentiC",
        "holdOnCentiC",
        "holdOffCentiC",
        "overshootCutoffCentiC",
        "holdKpPermillePerC",
        "holdKiPermillePerCTick",
        "holdBlendTicks",
        "approachLeadTicks",
        "holdLeadTicks",
    ];
    const SETTINGS_FIELDS: &[&str] = &[
        "tempFilterAlphaPermille",
        "warmupReenterCentiC",
        "approachMaxTicks",
        "approachMinPowerRatioPermille",
        "autoAdjustableWorkingFloorMv",
        "heaterCurrentReserveMa",
    ];

    let expected_point = expected.as_object().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "heater parameters must be a JSON object",
        )
    })?;
    for field in POINT_FIELDS {
        verify_thermal_control_readback_field(actual, expected_point, field)?;
    }
    let expected_settings = expected
        .get("settings")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "heater parameters missing settings",
            )
        })?;
    for field in SETTINGS_FIELDS {
        verify_thermal_control_readback_field(actual, expected_settings, field)?;
    }
    Ok(())
}

fn verify_thermal_control_readback_field(
    actual: &serde_json::Map<String, Value>,
    expected: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let expected_value = expected.get(field).and_then(Value::as_u64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("heater parameters missing integer field: {field}"),
        )
    })?;
    let actual_value = actual.get(field).and_then(Value::as_u64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("thermal control readback missing integer field: {field}"),
        )
    })?;
    if actual_value != expected_value {
        return Err(format!(
            "thermal control readback mismatch for {field}: expected {expected_value}, got {actual_value}"
        )
        .into());
    }
    Ok(())
}

fn thermal_self_test_runtime_body(heater_enabled: bool, target_temp_c: i16) -> Value {
    let mut body = json!({
        "heaterEnabled": heater_enabled,
        "targetTempC": target_temp_c,
    });
    if !heater_enabled {
        body["activeCoolingEnabled"] = json!(true);
    }
    body
}

fn thermal_source_summary_value(
    args: &ThermalSelfTestArgs,
    selection: &ThermalSourceSelection,
    source_voltage_mv: u16,
    source_current_ma: u16,
) -> Value {
    json!({
        "kind": args.source_kind.as_str(),
        "id": args.source_id,
        "deviceId": args.source_id,
        "mode": args.source_mode,
        "capabilityPowerWatts": args.source_power_watts,
        "selectedMode": args.profile_mode.as_str(),
        "resolvedBank": selection.resolved_bank,
        "detectedSourceClass": selection.detected_source_class,
        "detectedSourceClassBasis": selection.detected_source_class_basis,
        "preset": {
            "voltageMv": source_voltage_mv,
            "currentLimitMa": source_current_ma,
            "ppsEnabled": true,
            "pdFixedEnabled": true,
        },
        "url": args.source_url,
        "voltageMv": (args.source_mode == "manual-forced").then_some(source_voltage_mv),
        "currentLimitMa": (args.source_mode == "manual-forced").then_some(source_current_ma),
        "usbCPath": if args.source_mode == "manual-forced" { "forced-on" } else { "pd-auto" },
    })
}

fn thermal_source_class(source_voltage_mv: u16, source_current_ma: u16) -> &'static str {
    if source_voltage_mv >= 20_000 && source_current_ma >= 5_000 {
        "pps5a"
    } else {
        "pps3a"
    }
}

fn thermal_self_test_cooldown_runtime_body() -> Value {
    json!({
        "heaterEnabled": false,
        "activeCoolingEnabled": true,
        "thermalControlProfile": {
            "op": "clear_preview"
        }
    })
}

async fn arm_thermal_self_test_heater(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    heater_enabled: bool,
    target_temp_c: i16,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let status = request_leased(
        client,
        resolved,
        lease_id,
        Method::PUT,
        "/runtime",
        Some(thermal_self_test_runtime_body(
            heater_enabled,
            target_temp_c,
        )),
    )
    .await?;
    let readback_target = require_status_i32(&status, "targetTempC")?;
    if readback_target != i32::from(target_temp_c) {
        return Err(format!(
            "heater runtime readback target mismatch: expected {target_temp_c}, got {readback_target}"
        )
        .into());
    }
    if status.get("heaterEnabled").and_then(Value::as_bool) != Some(heater_enabled) {
        return Err(format!(
            "heater runtime readback enable mismatch: expected {heater_enabled}, got {}",
            status
                .get("heaterEnabled")
                .and_then(Value::as_bool)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".to_string())
        )
        .into());
    }
    if !heater_enabled && status.get("activeCoolingEnabled").and_then(Value::as_bool) != Some(true)
    {
        return Err(format!(
            "heater runtime readback activeCooling mismatch: expected true, got {}",
            status
                .get("activeCoolingEnabled")
                .and_then(Value::as_bool)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".to_string())
        )
        .into());
    }
    Ok(status)
}

async fn run_thermal_stage(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    samples_writer: &mut BufWriter<File>,
    run_id: &str,
    test_phase: &str,
    target_temp_c: i16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    heater_parameters: &Value,
    args: &ThermalSelfTestArgs,
    source_sampler: &mut BenchSourceTelemetrySampler,
    sample_index: &mut usize,
    initial_status: Option<Value>,
) -> Result<ThermalStageResult, Box<dyn std::error::Error + Send + Sync>> {
    let mut started = tokio::time::Instant::now();
    let stage_timeout = Duration::from_secs(args.stage_timeout_seconds.max(1));
    let warmup_timeout = Duration::from_secs(args.warmup_timeout_seconds.max(1));
    let mut deadline = started + stage_timeout;
    let hold_duration = Duration::from_secs(args.hold_seconds.max(1));
    let sample_interval = Duration::from_millis(effective_thermal_sample_interval_ms(
        args.sample_interval_ms,
    ));
    let control_target = thermal_candidate_point_from_heater_parameters(heater_parameters)?;
    let mut next_tick = started;
    let mut hold_tracker = ThermalHoldTracker::new(target_temp_c, hold_duration);
    let mut analyzer = ThermalStageAnalyzer::new(target_temp_c);
    let mut approach_guard =
        ThermalApproachGuardTracker::new(target_temp_c, control_target.hold_entry_centi_c);
    let mut full_speed_tracker = ThermalFullSpeedStableTracker::new(target_temp_c);
    let mut max_temp_c = f64::NEG_INFINITY;
    let mut stage_sample_count = 0usize;
    let mut recorded_samples = Vec::<ThermalReplayStageSample>::new();
    let mut stop_reason = "timeout";
    let mut terminal_runtime_drop_reason = None::<&'static str>;
    let mut last_uptime_seconds = None::<u64>;
    let mut sample_rate_tracker = ThermalSampleRateTracker::new();
    let mut heater_output_seen = false;
    let mut runtime_rearm_attempts_remaining = args.runtime_rearm_attempts;
    let mut next_status = initial_status;

    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        let (source_telemetry, source_telemetry_stale_ms) = source_sampler.snapshot().await?;
        let status = if let Some(status) = next_status.take() {
            status
        } else {
            match request_thermal_status_with_retry(client, resolved, lease_id).await {
                Ok(status) => status,
                Err(_) => {
                    stop_reason = "status_request_failed";
                    break;
                }
            }
        };
        let runtime_drop_reason =
            thermal_runtime_drop_reason(&status, target_temp_c, last_uptime_seconds);
        if let Some(reason) = runtime_drop_reason {
            let recoverable_sensor_fault =
                runtime_rearm_attempts_remaining > 0 && thermal_recoverable_sensor_fault(&status);
            let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            let sample = json!({
                "runId": run_id,
                "sampleIndex": *sample_index,
                "capturedAtUnixMs": current_unix_millis(),
                "elapsedMs": elapsed_ms,
                "testPhase": test_phase,
                "phase": "runtime_rearm",
                "targetTempC": target_temp_c,
                "source": {
                    "mode": args.source_mode,
                    "capabilityPowerWatts": args.source_power_watts,
                    "requestedVoltageMv": (args.source_mode == "manual-forced").then_some(source_voltage_mv),
                    "requestedCurrentLimitMa": (args.source_mode == "manual-forced").then_some(source_current_ma),
                },
                "sourceTelemetry": source_telemetry.to_value(),
                "sourceTelemetryStaleMs": source_telemetry_stale_ms,
                "heaterTelemetry": heater_telemetry_value(&status)?,
                "heaterParameters": heater_parameters,
                "runtimeDropReason": reason.as_str(),
                "runtimeRearmAttemptsRemaining": runtime_rearm_attempts_remaining,
                "status": status,
            });
            writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
            samples_writer.flush()?;
            *sample_index = sample_index.saturating_add(1);
            stage_sample_count = stage_sample_count.saturating_add(1);
            if recoverable_sensor_fault {
                runtime_rearm_attempts_remaining =
                    runtime_rearm_attempts_remaining.saturating_sub(1);
                next_status = Some(
                    arm_thermal_self_test_heater(client, resolved, lease_id, true, target_temp_c)
                        .await?,
                );
                started = tokio::time::Instant::now();
                deadline = started + stage_timeout;
                next_tick = started;
                hold_tracker = ThermalHoldTracker::new(target_temp_c, hold_duration);
                analyzer = ThermalStageAnalyzer::new(target_temp_c);
                approach_guard = ThermalApproachGuardTracker::new(
                    target_temp_c,
                    control_target.hold_entry_centi_c,
                );
                full_speed_tracker = ThermalFullSpeedStableTracker::new(target_temp_c);
                max_temp_c = f64::NEG_INFINITY;
                recorded_samples.clear();
                last_uptime_seconds = None;
                sample_rate_tracker = ThermalSampleRateTracker::new();
                heater_output_seen = false;
                continue;
            }
            stop_reason = reason.as_str();
            terminal_runtime_drop_reason = Some(reason.as_str());
            break;
        }
        last_uptime_seconds = status.get("uptimeSeconds").and_then(Value::as_u64);
        let current_temp_c = require_status_f64(&status, "currentTempC")?;
        let heater_output_percent =
            require_status_u64(&status, "heaterOutputPercent")?.min(u64::from(u8::MAX)) as u8;
        heater_output_seen |= heater_output_percent > 0;
        max_temp_c = max_temp_c.max(current_temp_c);
        let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let sample_rate = sample_rate_tracker.observe(elapsed_ms);
        let control_phase = status.get("heaterControlPhase").and_then(Value::as_str);
        let control_phase_in_hold = control_phase.is_some_and(|phase| phase == "hold");
        analyzer.observe(
            current_temp_c,
            heater_output_percent,
            elapsed_ms,
            control_phase_in_hold,
        );
        let observation =
            hold_tracker.observe(current_temp_c, elapsed_ms, now, control_phase_in_hold);
        let mut phase = match observation {
            ThermalHoldObservation::Warmup => "warmup",
            ThermalHoldObservation::Hold | ThermalHoldObservation::Completed => {
                if observation == ThermalHoldObservation::Completed {
                    stop_reason = "completed";
                }
                "hold"
            }
        };
        if let Some(control_phase) = control_phase {
            phase = control_phase;
        }
        if stop_reason == "timeout" && sample_rate.violation {
            stop_reason = "sample_rate_below_3hz";
        }
        if stop_reason == "timeout"
            && !heater_output_seen
            && elapsed_ms >= THERMAL_HEATER_OUTPUT_START_TIMEOUT_MS
            && current_temp_c < f64::from(target_temp_c) - 1.0
        {
            stop_reason = "heater_no_output";
        }
        let _guard_stop_reason = if stop_reason == "timeout" {
            approach_guard.observe(current_temp_c, elapsed_ms, control_phase)
        } else {
            None
        };
        let full_speed_stop_reason = if stop_reason == "timeout" {
            match full_speed_tracker.observe(current_temp_c, elapsed_ms, control_phase) {
                ThermalFullSpeedStableObservation::Failed(reason) => Some(reason),
                _ => None,
            }
        } else {
            None
        };
        if stop_reason == "timeout"
            && full_speed_tracker.warmup_exited_at_ms.is_none()
            && started.elapsed() >= warmup_timeout
        {
            stop_reason = "warmup_timeout";
        }
        if stop_reason == "timeout" {
            if let Some(reason) = full_speed_stop_reason {
                stop_reason = reason;
            }
        }
        let sample = json!({
            "runId": run_id,
            "sampleIndex": *sample_index,
            "capturedAtUnixMs": current_unix_millis(),
            "elapsedMs": elapsed_ms,
            "testPhase": test_phase,
            "phase": phase,
            "targetTempC": target_temp_c,
            "source": {
                "mode": args.source_mode,
                "requestedVoltageMv": (args.source_mode == "manual-forced").then_some(source_voltage_mv),
                "requestedCurrentLimitMa": (args.source_mode == "manual-forced").then_some(source_current_ma),
            },
            "sourceTelemetry": source_telemetry.to_value(),
            "sourceTelemetryStaleMs": source_telemetry_stale_ms,
            "heaterTelemetry": heater_telemetry_value(&status)?,
            "heaterParameters": heater_parameters,
            "sampling": {
                "intervalMs": sample_rate.interval_ms,
                "rollingRateHz": sample_rate.rolling_rate_hz,
                "minimumRateHz": THERMAL_MIN_SAMPLE_RATE_HZ,
                "windowMs": THERMAL_SAMPLE_RATE_WINDOW_MS,
                "rateViolation": sample_rate.violation,
            },
            "status": status,
        });
        writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
        samples_writer.flush()?;
        recorded_samples.push(ThermalReplayStageSample {
            elapsed_ms,
            current_temp_c,
            heater_output_percent,
            control_phase: control_phase.map(ToOwned::to_owned),
            control_phase_in_hold,
            source_voltage_mv: Some(source_telemetry.voltage_mv),
            source_current_ma: Some(source_telemetry.current_ma),
            source_power_mw: Some(source_telemetry.power_mw),
        });
        *sample_index = sample_index.saturating_add(1);
        stage_sample_count = stage_sample_count.saturating_add(1);
        // Every non-timeout reason is terminal. fullSpeedToStable uses the current
        // 5A target-temperature budget: <=150C has 10s, >150C has 5s.
        if stop_reason != "timeout" {
            break;
        }
        next_tick += sample_interval;
        tokio::time::sleep_until(next_tick).await;
    }
    if stop_reason != "completed" {
        let _ =
            arm_thermal_self_test_heater(client, resolved, lease_id, false, target_temp_c).await?;
    }

    let guard = approach_guard.finalize();
    let full_speed_to_stable = full_speed_tracker.finalize();
    let mut analysis = if max_temp_c.is_finite() {
        analyzer.finalize(max_temp_c)
    } else {
        ThermalStageAnalysis::default()
    };
    thermal_stage_populate_approach_curve_analysis(&mut analysis, &recorded_samples, target_temp_c);

    Ok(ThermalStageResult {
        target_temp_c,
        rise_time_ms: hold_tracker
            .rise_time_ms()
            .unwrap_or_else(|| started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
        max_overshoot_c: (max_temp_c - f64::from(target_temp_c)).max(0.0),
        hold_peak_to_peak_c: hold_tracker.peak_to_peak_c(),
        sample_count: stage_sample_count,
        stop_reason,
        terminal_runtime_drop_reason,
        analysis,
        guard,
        full_speed_to_stable,
    })
}

async fn wait_for_cooldown(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    cooldown_temp_c: f64,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let status =
            request_leased(client, resolved, lease_id, Method::GET, "/status", None).await?;
        let current_temp_c = require_status_f64(&status, "currentTempC")?;
        if current_temp_c <= cooldown_temp_c {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "thermal self-test requires cooldown to <= {cooldown_temp_c:.1}C, got {current_temp_c:.1}C"
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(THERMAL_COOLDOWN_POLL_INTERVAL_MS)).await;
    }
}

fn thermal_stage_can_continue_tuning(stage: &ThermalStageResult) -> bool {
    matches!(
        stage.stop_reason,
        "completed"
            | "warmup_timeout"
            | "full_speed_to_stable_timeout"
            | "approach_threshold_timeout"
            | "approach_hold_timeout"
            | "approach_reentered_warmup"
    )
}

fn thermal_stage_can_tune(stage: &ThermalStageResult) -> bool {
    matches!(
        stage.stop_reason,
        "completed"
            | "timeout"
            | "warmup_timeout"
            | "full_speed_to_stable_timeout"
            | "approach_threshold_timeout"
            | "approach_hold_timeout"
            | "approach_reentered_warmup"
    )
}

fn validate_thermal_applied_results(
    applied: &[ThermalStageResult],
    expected_targets_c: &[i16],
    evaluation_mode: ThermalSelfTestEvaluationMode,
) -> Value {
    let mut failures = Vec::new();
    for target_temp_c in expected_targets_c {
        if !applied
            .iter()
            .any(|stage| stage.target_temp_c == *target_temp_c)
        {
            failures.push(json!({
                "targetTempC": target_temp_c,
                "phase": "applied",
                "reason": "missing_stage",
            }));
        }
    }
    for applied_stage in applied {
        if applied_stage.stop_reason != "completed" {
            failures.push(json!({
                "targetTempC": applied_stage.target_temp_c,
                "phase": "applied",
                "reason": "incomplete_stage",
                "stopReason": applied_stage.stop_reason,
                "guard": {
                    "approachStartedAtMs": applied_stage.guard.approach_started_at_ms,
                    "holdThresholdCrossedAtMs": applied_stage.guard.hold_threshold_crossed_at_ms,
                    "firstHoldAtMs": applied_stage.guard.first_hold_at_ms,
                    "warmupReenteredAtMs": applied_stage.guard.warmup_reentered_at_ms,
                },
            }));
        }
        if evaluation_mode.enforces_stage_limits() && applied_stage.max_overshoot_c > 3.0 {
            failures.push(json!({
                "targetTempC": applied_stage.target_temp_c,
                "reason": "overshoot",
                "value": applied_stage.max_overshoot_c,
                "limit": 3.0,
            }));
        }
        if evaluation_mode.enforces_stage_limits() && applied_stage.hold_peak_to_peak_c > 3.0 {
            failures.push(json!({
                "targetTempC": applied_stage.target_temp_c,
                "reason": "hold_p2p",
                "value": applied_stage.hold_peak_to_peak_c,
                "limit": 3.0,
            }));
        }
        if evaluation_mode.enforces_stage_limits() {
            let limit_ms = ThermalFullSpeedStableTracker::settle_limit_ms_for_target(
                applied_stage.target_temp_c,
            );
            match applied_stage.full_speed_to_stable.settle_time_ms {
                Some(value) if value <= limit_ms => {}
                Some(value) => failures.push(json!({
                    "targetTempC": applied_stage.target_temp_c,
                    "reason": "full_speed_to_stable",
                    "value": value,
                    "limit": limit_ms,
                    "failureReason": applied_stage.full_speed_to_stable.failure_reason,
                })),
                None => failures.push(json!({
                    "targetTempC": applied_stage.target_temp_c,
                    "reason": "full_speed_to_stable_missing",
                    "limit": limit_ms,
                    "warmupExitedAtMs": applied_stage.full_speed_to_stable.warmup_exited_at_ms,
                    "stableWindowStartedAtMs": applied_stage.full_speed_to_stable.stable_window_started_at_ms,
                    "failureReason": applied_stage.full_speed_to_stable.failure_reason,
                })),
            }
        }
    }
    json!({
        "passed": failures.is_empty() && !expected_targets_c.is_empty(),
        "expectedTargetsC": expected_targets_c,
        "failures": failures,
    })
}

fn read_bench_source_live_telemetry(
    source_kind: BenchSourceKind,
    source_url: &str,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    match source_kind {
        BenchSourceKind::Isolapurr => read_isolapurr_live_telemetry(source_url),
    }
}

fn validate_thermal_bench_source_tools(
    source_kind: BenchSourceKind,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match source_kind {
        BenchSourceKind::Isolapurr => validate_isolapurr_tools(),
    }
}

async fn set_thermal_bench_source_output_auto(
    client: &Client,
    source_kind: BenchSourceKind,
    source_url: &str,
    source_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match source_kind {
        BenchSourceKind::Isolapurr => {
            set_isolapurr_output_auto(client, source_url, source_id).await
        }
    }
}

async fn prepare_thermal_bench_source(
    client: &Client,
    source_kind: BenchSourceKind,
    source_url: &str,
    source_id: &str,
    source_mode: &str,
    profile_mode: ThermalProfileMode,
    source_power_watts: u16,
    voltage_mv: u16,
    current_limit_ma: u16,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    match source_kind {
        BenchSourceKind::Isolapurr => {
            prepare_isolapurr_thermal_source(
                client,
                source_url,
                source_id,
                source_mode,
                profile_mode,
                source_power_watts,
                voltage_mv,
                current_limit_ma,
            )
            .await
        }
    }
}

fn read_isolapurr_live_telemetry(
    source_url: &str,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    let mut last_error = None::<String>;
    for attempt in 1..=3 {
        let result = isolapurr_cli_json_with_timeout(
            source_url,
            &["power", "show"],
            Duration::from_millis(750),
        )
        .and_then(|power| parse_isolapurr_live_telemetry(&power).map_err(Into::into));
        match result {
            Ok(telemetry) => return Ok(telemetry),
            Err(error) => last_error = Some(error.to_string()),
        }
        if attempt < 3 {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Err(last_error
        .unwrap_or_else(|| "isolapurr USB-C telemetry unavailable".to_string())
        .into())
}

fn validate_isolapurr_tools() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for tool in ["isolapurr"] {
        let output = ProcessCommand::new(tool)
            .arg("--help")
            .output()
            .map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!(
                        "{tool} --help failed to start; install the required host tooling: {error}"
                    ),
                )
            })?;
        if !output.status.success() {
            return Err(format!("{tool} --help exited with {}", output.status).into());
        }
    }
    Ok(())
}

fn parse_isolapurr_live_telemetry(ports: &Value) -> Result<BenchSourceLiveTelemetry, String> {
    let port_c = ports
        .get("ports")
        .and_then(Value::as_array)
        .or_else(|| {
            ports
                .get("ports")
                .and_then(|ports| ports.get("ports"))
                .and_then(Value::as_array)
        })
        .and_then(|ports| {
            ports.iter().find(|port| {
                port.get("portId").and_then(Value::as_str) == Some("port_c")
                    || port.get("label").and_then(Value::as_str) == Some("USB-C")
            })
        })
        .ok_or_else(|| "isolapurr ports missing USB-C telemetry".to_string())?;
    let telemetry = port_c
        .get("telemetry")
        .and_then(Value::as_object)
        .or_else(|| port_c.get("telemetry_raw").and_then(Value::as_object))
        .ok_or_else(|| "isolapurr USB-C telemetry missing object".to_string())?;
    let status = json_str_any(telemetry, &["status"]).unwrap_or("unknown");
    let state = port_c.get("state").cloned().unwrap_or(Value::Null);
    let voltage_mv = json_u64_any(telemetry, &["voltage_mv", "voltageMv"]).ok_or_else(|| {
        format!("isolapurr USB-C telemetry missing voltage status={status} state={state}")
    })?;
    let current_ma = json_u64_any(telemetry, &["current_ma", "currentMa"]).ok_or_else(|| {
        format!("isolapurr USB-C telemetry missing current status={status} state={state}")
    })?;
    let power_mw = json_u64_any(telemetry, &["power_mw", "powerMw"]).ok_or_else(|| {
        format!("isolapurr USB-C telemetry missing power status={status} state={state}")
    })?;
    let sample_uptime_ms = json_u64_any(telemetry, &["sample_uptime_ms", "sampleUptimeMs"])
        .ok_or_else(|| {
            format!("isolapurr USB-C telemetry missing sample uptime status={status} state={state}")
        })?;
    Ok(BenchSourceLiveTelemetry {
        voltage_mv,
        current_ma,
        power_mw,
        sample_uptime_ms,
        status: status.to_string(),
    })
}

async fn set_isolapurr_output_auto(
    _client: &Client,
    source_url: &str,
    device_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(source_url, device_id)?;
    let mut config = read_isolapurr_power_config(source_url)?;
    if !isolapurr_power_config_usb_c_path_is_default(&config) {
        let response = isolapurr_cli_json(
            source_url,
            &["power", "output", "manual", "--usb-c-path", "automatic"],
        )?;
        if !isolapurr_cli_write_succeeded(&response)
            && !isolapurr_power_config_usb_c_path_is_default(&response)
        {
            return Err(format!(
                "isolapurr auto output path-normalization did not acknowledge success source_url={source_url}"
            )
            .into());
        }
        config = read_isolapurr_power_config(source_url)?;
    }
    if !isolapurr_power_config_value_is_auto(&config) {
        let response = isolapurr_cli_json(source_url, &["power", "output", "auto"])?;
        if !isolapurr_cli_write_succeeded(&response)
            && !isolapurr_power_config_value_is_auto(&response)
        {
            return Err(format!(
                "isolapurr auto output command did not acknowledge success source_url={source_url}"
            )
            .into());
        }
        config = read_isolapurr_power_config(source_url)?;
    }
    if !isolapurr_power_config_value_is_auto(&config) {
        return Err(format!(
            "isolapurr power output auto readback mismatch for source_url={source_url}"
        )
        .into());
    }
    if !isolapurr_power_config_path_is_automatic(&config) {
        return Err(format!(
            "isolapurr power output auto left USB-C path in a non-automatic state for source_url={source_url}"
        )
        .into());
    }
    Ok(())
}

async fn prepare_isolapurr_thermal_source(
    client: &Client,
    source_url: &str,
    device_id: &str,
    source_mode: &str,
    profile_mode: ThermalProfileMode,
    source_power_watts: u16,
    voltage_mv: u16,
    current_limit_ma: u16,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    if source_mode == "manual-forced" {
        set_isolapurr_output_manual(client, source_url, device_id, voltage_mv, current_limit_ma)
            .await
    } else {
        if profile_mode.explicit_bank().is_some() {
            ensure_isolapurr_thermal_capability(source_url, device_id, source_power_watts)?;
        } else {
            ensure_isolapurr_auto_thermal_capability(source_url, device_id)?;
        }
        set_isolapurr_output_auto(client, source_url, device_id).await?;
        let telemetry = ensure_isolapurr_live_telemetry_ready(source_url)?;
        validate_isolapurr_ready_voltage(&telemetry)?;
        Ok(telemetry)
    }
}

async fn prepare_thermal_source_and_lease(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    source_kind: BenchSourceKind,
    source_url: &str,
    source_id: &str,
    source_mode: &str,
    profile_mode: ThermalProfileMode,
    source_power_watts: u16,
    voltage_mv: u16,
    current_limit_ma: u16,
) -> Result<(BenchSourceLiveTelemetry, Lease), Box<dyn std::error::Error + Send + Sync>> {
    let telemetry = prepare_thermal_bench_source(
        client,
        source_kind,
        source_url,
        source_id,
        source_mode,
        profile_mode,
        source_power_watts,
        voltage_mv,
        current_limit_ma,
    )
    .await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    match create_ready_thermal_lease(client, resolved).await {
        Ok((lease, _status)) => Ok((telemetry, lease)),
        Err(error) => {
            match set_thermal_bench_source_output_auto(client, source_kind, source_url, source_id)
                .await
            {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!(
                    "{error}; {} cleanup after lease failure also failed: {cleanup_error}",
                    source_kind.as_str()
                )
                .into()),
            }
        }
    }
}

fn ensure_isolapurr_thermal_capability(
    source_url: &str,
    device_id: &str,
    source_power_watts: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(source_url, device_id)?;
    let required_power_watts = u64::from(source_power_watts);
    let requires_pps_5a = required_power_watts == THERMAL_SOURCE_100W_POWER_WATTS;
    let mut config = read_isolapurr_power_config(source_url)?;
    if !isolapurr_power_config_has_thermal_capability(
        &config,
        required_power_watts,
        requires_pps_5a,
    ) {
        let power_watts = required_power_watts.to_string();
        let pd_pps_5a = if requires_pps_5a { "true" } else { "false" };
        let response = isolapurr_cli_json(
            source_url,
            &[
                "power",
                "source-capability",
                "set",
                "--power-watts",
                &power_watts,
                "--pd",
                "true",
                "--pps3-limit-ma",
                "5000",
                "--pd-pps-5a",
                pd_pps_5a,
                "--pps",
                "true",
            ],
        )?;
        if !isolapurr_cli_write_succeeded(&response)
            && !isolapurr_power_config_has_thermal_capability(
                response.get("config").unwrap_or(&response),
                required_power_watts,
                requires_pps_5a,
            )
        {
            return Err("isolapurr source capability command did not acknowledge success".into());
        }
        config = read_isolapurr_power_config(source_url)?;
    }
    if !isolapurr_power_config_has_thermal_capability(
        &config,
        required_power_watts,
        requires_pps_5a,
    ) {
        return Err(
            format!(
                "isolapurr source capability readback must confirm {required_power_watts}W, PD Fixed, and PPS"
            )
            .into(),
        );
    }
    Ok(())
}

fn ensure_isolapurr_auto_thermal_capability(
    source_url: &str,
    device_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(source_url, device_id)?;
    let config = read_isolapurr_power_config(source_url)?;
    let source_class = isolapurr_configured_thermal_source_class(&config).ok_or_else(|| {
        format!(
            "isolapurr source capability readback must advertise PPS with 20V coverage for auto thermal mode source_url={source_url}"
        )
    })?;
    if source_class != "pps3a" && source_class != "pps5a" {
        return Err(format!("unsupported isolapurr thermal source class: {source_class}").into());
    }
    Ok(())
}

fn read_isolapurr_configured_source_class(
    source_url: &str,
    device_id: &str,
) -> Result<&'static str, Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(source_url, device_id)?;
    let config = read_isolapurr_power_config(source_url)?;
    isolapurr_configured_thermal_source_class(&config).ok_or_else(|| {
        format!(
            "isolapurr source capability readback must advertise PPS with 20V coverage source_url={source_url}"
        )
        .into()
    })
}

fn isolapurr_configured_thermal_source_class(config: &Value) -> Option<&'static str> {
    let capability = config.get("capability").unwrap_or(config);
    let fixed_voltages = capability
        .pointer("/pd/fixed_voltages_mv")
        .or_else(|| capability.pointer("/pd/fixedVoltagesMv"))
        .and_then(Value::as_array)?;
    let pps_enabled = capability.pointer("/pd/pps").and_then(Value::as_bool) == Some(true);
    let pd_enabled = capability.pointer("/protocols/pd").and_then(Value::as_bool) == Some(true);
    let covers_20v = fixed_voltages
        .iter()
        .filter_map(Value::as_u64)
        .any(|voltage_mv| voltage_mv >= 20_000);
    if !pps_enabled || !pd_enabled || !covers_20v {
        return None;
    }
    let pps_max_ma = json_u64_any(
        capability.as_object().unwrap_or(&serde_json::Map::new()),
        &["pps3_limit_ma", "pps3LimitMa"],
    )
    .or_else(|| {
        capability
            .pointer("/current/pps3_limit_ma")
            .or_else(|| capability.pointer("/current/pps3LimitMa"))
            .or_else(|| capability.pointer("/pd/pps3_limit_ma"))
            .or_else(|| capability.pointer("/pd/pps3LimitMa"))
            .and_then(Value::as_u64)
    })
    .unwrap_or(0);
    Some(if pps_max_ma >= 5_000 {
        "pps5a"
    } else {
        "pps3a"
    })
}

fn isolapurr_power_config_has_thermal_capability(
    config: &Value,
    required_power_watts: u64,
    requires_pps_5a: bool,
) -> bool {
    let capability = config.get("capability").unwrap_or(config);
    let pps3_limit_ma = json_u64_any(
        capability.as_object().unwrap_or(&serde_json::Map::new()),
        &["pps3_limit_ma", "pps3LimitMa"],
    )
    .or_else(|| {
        capability
            .pointer("/current/pps3_limit_ma")
            .or_else(|| capability.pointer("/current/pps3LimitMa"))
            .or_else(|| capability.pointer("/pd/pps3_limit_ma"))
            .or_else(|| capability.pointer("/pd/pps3LimitMa"))
            .and_then(Value::as_u64)
    });
    let pd_pps_5a = capability
        .pointer("/current/pd_pps_5a")
        .or_else(|| capability.pointer("/current/pdPps5a"))
        .or_else(|| capability.pointer("/pd/pd_pps_5a"))
        .or_else(|| capability.pointer("/pd/pdPps5a"))
        .or_else(|| capability.get("pd_pps_5a"))
        .or_else(|| capability.get("pdPps5a"))
        .and_then(Value::as_bool);
    json_u64_any(
        capability.as_object().unwrap_or(&serde_json::Map::new()),
        &["power_watts", "powerWatts"],
    ) == Some(required_power_watts)
        && capability.pointer("/protocols/pd").and_then(Value::as_bool) == Some(true)
        && capability.pointer("/pd/pps").and_then(Value::as_bool) == Some(true)
        && capability
            .pointer("/pd/fixed_voltages_mv")
            .or_else(|| capability.pointer("/pd/fixedVoltagesMv"))
            .and_then(Value::as_array)
            .is_some_and(|voltages| !voltages.is_empty())
        && (!requires_pps_5a || (pps3_limit_ma >= Some(5_000) && pd_pps_5a == Some(true)))
}

fn validate_isolapurr_ready_voltage(
    telemetry: &BenchSourceLiveTelemetry,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if telemetry.status != "ok" {
        return Err(format!(
            "isolapurr USB-C output is not ready status={} voltage={}mV current={}mA",
            telemetry.status, telemetry.voltage_mv, telemetry.current_ma
        )
        .into());
    }
    if telemetry.voltage_mv <= THERMAL_SOURCE_MIN_READY_VOLTAGE_MV {
        return Err(format!(
            "isolapurr USB-C output is not above 5V actual={}mV",
            telemetry.voltage_mv
        )
        .into());
    }
    Ok(())
}

async fn set_isolapurr_output_manual(
    _client: &Client,
    source_url: &str,
    device_id: &str,
    voltage_mv: u16,
    current_limit_ma: u16,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(source_url, device_id)?;
    let mut config = read_isolapurr_power_config(source_url)?;
    if !isolapurr_power_config_value_matches_manual(&config, voltage_mv, current_limit_ma) {
        let response = isolapurr_cli_json(
            source_url,
            &[
                "power",
                "output",
                "manual",
                "--voltage-mv",
                &voltage_mv.to_string(),
                "--current-limit-ma",
                &current_limit_ma.to_string(),
                "--usb-c-path",
                "forced-on",
            ],
        )?;
        if !isolapurr_cli_write_succeeded(&response)
            && !isolapurr_power_config_value_matches_manual(&response, voltage_mv, current_limit_ma)
        {
            return Err(format!(
                "isolapurr manual output command did not acknowledge success source_url={source_url}"
            )
            .into());
        }
        config = read_isolapurr_power_config(source_url)?;
    }
    if !isolapurr_power_config_value_matches_manual(&config, voltage_mv, current_limit_ma) {
        return Err(format!(
            "isolapurr power output manual readback mismatch for source_url={source_url}"
        )
        .into());
    }
    ensure_isolapurr_live_telemetry_ready(source_url)
}

fn validate_isolapurr_device_identity(
    source_url: &str,
    device_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let status = isolapurr_cli_json(source_url, &["status"])?;
    if !isolapurr_status_identity_matches(&status, device_id) {
        let actual = isolapurr_status_device_id(&status).unwrap_or("unknown");
        return Err(format!(
            "isolapurr identity mismatch source_url={source_url} expected_device_id={device_id} actual_device_id={actual}"
        )
        .into());
    }
    Ok(())
}

fn isolapurr_power_config_value_matches_manual(
    config: &Value,
    voltage_mv: u16,
    current_limit_ma: u16,
) -> bool {
    config
        .as_object()
        .and_then(|config| json_str_any(config, &["tps_mode", "tpsMode"]))
        .is_some_and(|mode| mode == "manual")
        && config
            .get("manual")
            .and_then(Value::as_object)
            .is_some_and(|manual| {
                let manual_current_ma =
                    json_u64_any(manual, &["current_limit_ma", "currentLimitMa"]);
                let current_matches = manual_current_ma == Some(u64::from(current_limit_ma))
                    // A 100W source cannot sustain 21V * 5A (105W). The released
                    // IsolaPurr firmware quantizes the resulting 100W limit to 4.75A.
                    || (voltage_mv == 21_000
                        && current_limit_ma == 5_000
                        && manual_current_ma == Some(4_750)
                        && isolapurr_power_config_has_thermal_capability(
                            config,
                            THERMAL_SOURCE_100W_POWER_WATTS,
                            true,
                        ));
                json_u64_any(manual, &["voltage_mv", "voltageMv"]) == Some(u64::from(voltage_mv))
                    && current_matches
                    && matches!(
                        json_str_any(manual, &["usb_c_path_mode", "usbCPathMode"]),
                        Some("force" | "forced-on")
                    )
                    && matches!(
                        json_str_any(manual, &["path_policy", "pathPolicy"]),
                        Some("force_open" | "force-open")
                    )
            })
}

fn isolapurr_status_identity_matches(status: &Value, device_id: &str) -> bool {
    isolapurr_status_device_id(status) == Some(device_id)
}

fn isolapurr_status_device_id(status: &Value) -> Option<&str> {
    status
        .get("device")
        .or_else(|| status.get("result")?.get("device"))
        .and_then(Value::as_object)
        .and_then(|device| json_str_any(device, &["device_id", "deviceId"]))
}

fn isolapurr_power_config_value_is_auto(config: &Value) -> bool {
    config
        .as_object()
        .and_then(|config| json_str_any(config, &["tps_mode", "tpsMode"]))
        .is_some_and(|mode| mode == "auto_follow" || mode == "autoFollow")
}

fn isolapurr_power_config_usb_c_path_is_default(config: &Value) -> bool {
    config
        .get("manual")
        .and_then(Value::as_object)
        .is_some_and(|manual| {
            matches!(
                json_str_any(manual, &["usb_c_path_mode", "usbCPathMode"]),
                Some("default" | "automatic")
            )
        })
}

fn isolapurr_power_config_path_is_automatic(config: &Value) -> bool {
    isolapurr_power_config_usb_c_path_is_default(config)
        && config
            .get("manual")
            .and_then(Value::as_object)
            .is_some_and(|manual| {
                matches!(
                    json_str_any(manual, &["path_policy", "pathPolicy"]),
                    Some("auto")
                )
            })
}

fn read_isolapurr_power_config(
    source_url: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let response = isolapurr_cli_json(source_url, &["power", "config", "show"])?;
    Ok(response.get("config").cloned().unwrap_or(response))
}

fn ensure_isolapurr_live_telemetry_ready(
    source_url: &str,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    let mut last_error = None::<String>;
    for attempt in 1..=6 {
        let power = isolapurr_cli_json(source_url, &["power", "show"])?;
        match parse_isolapurr_live_telemetry(&power) {
            Ok(telemetry) => return Ok(telemetry),
            Err(error) => {
                last_error = Some(error.clone());
                if attempt < 6 {
                    std::thread::sleep(Duration::from_millis(250 * attempt as u64));
                }
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| "isolapurr USB-C telemetry unavailable".to_string())
        .into())
}

fn isolapurr_cli_json(
    source_url: &str,
    args: &[&str],
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    isolapurr_cli_json_with_timeout(source_url, args, Duration::from_secs(5))
}

fn isolapurr_cli_json_with_timeout(
    source_url: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut command = ProcessCommand::new("isolapurr");
    command.arg("--json");
    command.args(args);
    command.args(["--url", source_url]);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let deadline = StdInstant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if StdInstant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "isolapurr {} timed out after {}ms",
                args.join(" "),
                timeout.as_millis()
            )
            .into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_end(&mut stdout)?;
    }
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_end(&mut stderr)?;
    }
    if !status.success() {
        return Err(format!(
            "isolapurr {} exited with {}; stderr={}",
            args.join(" "),
            status,
            String::from_utf8_lossy(&stderr).trim()
        )
        .into());
    }
    let stdout = String::from_utf8(stdout)?;
    serde_json::from_str(stdout.trim()).map_err(Into::into)
}

fn isolapurr_cli_write_succeeded(response: &Value) -> bool {
    (response.get("ok").and_then(Value::as_bool) == Some(true)
        || response.get("accepted").and_then(Value::as_bool) == Some(true))
        && response.get("error").is_none_or(Value::is_null)
}

fn json_u64_any<'a>(object: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_u64))
}

fn json_str_any<'a>(object: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
}

fn parse_calibration_channel(
    value: &str,
) -> Result<&'static str, Box<dyn std::error::Error + Send + Sync>> {
    match value {
        "rtd" | "rtd-adc" | "temp" | "temperature" => Ok("rtd_adc"),
        "vin" | "vin-adc" | "voltage" | "power" => Ok("vin_adc"),
        _ => Err("calibration channel must be rtd-adc or vin-adc".into()),
    }
}

fn parse_reference_vin_mv(
    millivolts: Option<u32>,
    volts: Option<&str>,
) -> Result<Option<u32>, Box<dyn std::error::Error + Send + Sync>> {
    if millivolts.is_some() && volts.is_some() {
        return Err("use either --reference-vin-mv or --reference-vin-volts, not both".into());
    }
    if let Some(millivolts) = millivolts {
        return Ok(Some(millivolts));
    }
    volts.map(parse_voltage_to_mv).transpose()
}

#[derive(Debug, Clone)]
struct CalibrationSeriesStats {
    count: u64,
    min: f64,
    max: f64,
    sum: f64,
    first: f64,
    last: f64,
}

impl CalibrationSeriesStats {
    fn new(value: f64) -> Self {
        Self {
            count: 1,
            min: value,
            max: value,
            sum: value,
            first: value,
            last: value,
        }
    }

    fn observe(&mut self, value: f64) {
        self.count = self.count.saturating_add(1);
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.sum += value;
        self.last = value;
    }

    fn to_value(&self) -> Value {
        json!({
            "count": self.count,
            "min": self.min,
            "max": self.max,
            "avg": self.sum / self.count.max(1) as f64,
            "first": self.first,
            "last": self.last,
        })
    }
}

fn observe_series(stats: &mut Option<CalibrationSeriesStats>, value: f64) {
    if let Some(stats) = stats.as_mut() {
        stats.observe(value);
    } else {
        *stats = Some(CalibrationSeriesStats::new(value));
    }
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn slugify_path_component(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.starts_with('-') {
        slug.remove(0);
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "run".to_string()
    } else {
        slug
    }
}

fn require_status_f64(
    status: &Value,
    key: &str,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    status.get(key).and_then(Value::as_f64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status missing numeric field: {key}"),
        )
        .into()
    })
}

fn require_status_u64(
    status: &Value,
    key: &str,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    status.get(key).and_then(Value::as_u64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status missing integer field: {key}"),
        )
        .into()
    })
}

fn require_status_u16(
    status: &Value,
    key: &str,
) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let value = require_status_u64(status, key)?;
    u16::try_from(value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status field out of range: {key}"),
        )
        .into()
    })
}

fn require_status_bool(
    status: &Value,
    key: &str,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    status.get(key).and_then(Value::as_bool).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status missing boolean field: {key}"),
        )
        .into()
    })
}

fn require_status_i32(
    status: &Value,
    key: &str,
) -> Result<i32, Box<dyn std::error::Error + Send + Sync>> {
    let value = status.get(key).and_then(Value::as_i64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status missing integer field: {key}"),
        )
    })?;
    i32::try_from(value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status field out of range: {key}"),
        )
        .into()
    })
}

fn status_snapshot(status: &Value) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let heater_output_percent = require_status_u64(status, "heaterOutputPercent")?;
    Ok(json!({
        "mode": status
            .get("mode")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: mode"))?,
        "heaterEnabled": status
            .get("heaterEnabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: heaterEnabled"))?,
        "heaterOutputPercent": heater_output_percent,
        "heaterPhysicalOutputPercent": status
            .get("heaterPhysicalOutputPercent")
            .and_then(Value::as_u64)
            .unwrap_or(heater_output_percent),
        "currentTempC": require_status_f64(status, "currentTempC")?,
        "targetTempC": require_status_i32(status, "targetTempC")?,
        "voltageMv": require_status_u64(status, "voltageMv")?,
        "currentMa": require_status_u64(status, "currentMa")?,
        "boardTempCenti": require_status_i32(status, "boardTempCenti")?,
        "rtdRawAdcMv": require_status_u16(status, "rtdRawAdcMv")?,
        "vinRawAdcMv": require_status_u16(status, "vinRawAdcMv")?,
        "pdRequestMv": require_status_u16(status, "pdRequestMv")?,
        "pdContractMv": require_status_u16(status, "pdContractMv")?,
        "pdState": status
            .get("pdState")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: pdState"))?,
        "activeCoolingEnabled": status
            .get("activeCoolingEnabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: activeCoolingEnabled"))?,
        "fanDisplayState": status
            .get("fanDisplayState")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: fanDisplayState"))?,
        "fanEnabled": status
            .get("fanEnabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: fanEnabled"))?,
        "fanPwmPermille": require_status_u64(status, "fanPwmPermille")?,
    }))
}

async fn collect_calibration_run(
    client: &Client,
    default_devd: &str,
    args: CalibrationCollectArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let resolved = resolve_target(args.target, default_devd)?;
    let source_current_ma = parse_pps_amps(&args.source_current_a)?;
    let run_started_unix_ms = current_unix_millis();
    let run_id = format!(
        "cal-{}-{}-{}mA",
        run_started_unix_ms,
        slugify_path_component(&resolved.device),
        source_current_ma
    );
    let run_dir = args.output_dir.join(&run_id);
    fs::create_dir_all(&run_dir)?;
    let samples_path = run_dir.join("samples.ndjson");
    let summary_path = run_dir.join("run.json");
    let mut samples_writer = BufWriter::new(File::create(&samples_path)?);

    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());

    let mut stop_reason = None::<&'static str>;
    let mut threshold_sample_index = None::<usize>;
    let mut stopped_sample_index = None::<usize>;
    let mut sample_index = 0usize;
    let mut samples_count = 0usize;
    let mut current_temp_stats: Option<CalibrationSeriesStats> = None;
    let mut voltage_stats: Option<CalibrationSeriesStats> = None;
    let mut current_ma_stats: Option<CalibrationSeriesStats> = None;
    let mut heater_output_stats: Option<CalibrationSeriesStats> = None;
    let mut board_temp_stats: Option<CalibrationSeriesStats> = None;
    let mut rtd_raw_stats: Option<CalibrationSeriesStats> = None;
    let mut vin_raw_stats: Option<CalibrationSeriesStats> = None;
    let mut first_status_snapshot: Option<Value> = None;
    let mut last_status_snapshot: Option<Value> = None;
    let mut heater_started = false;
    let mut heater_stopped = false;
    let mut final_status_snapshot = None::<Value>;
    let mut loop_started = tokio::time::Instant::now();
    let sample_interval = Duration::from_millis(args.sample_interval_ms.max(1));
    let max_runtime = Duration::from_secs(args.max_runtime_seconds.max(1));

    let collect_result = async {
        if !args.dry_run {
            let initial_status = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::GET,
                "/status",
                None,
            )
            .await?;
            let initial_current_temp = require_status_f64(&initial_status, "currentTempC")?;
            if initial_current_temp > 40.0 {
                return Err(format!(
                    "calibration collect requires room-temperature start (<= 40C), got {initial_current_temp:.1}C"
                )
                .into());
            }
            let body = json!({
                "heaterEnabled": true,
                "targetTempC": args.target_temp_c,
            });
            request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::PUT,
                "/runtime",
                Some(body),
            )
            .await?;
            heater_started = true;
            let readback = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::GET,
                "/status",
                None,
            )
            .await?;
            let readback_target = require_status_i32(&readback, "targetTempC")?;
            if !readback
                .get("heaterEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || readback_target != args.target_temp_c as i32
            {
                return Err("heater start readback did not match requested runtime state".into());
            }
        }

        loop_started = tokio::time::Instant::now();
        let deadline = loop_started + max_runtime;
        let mut next_tick = loop_started;

        loop {
            if tokio::time::Instant::now() >= deadline {
                stop_reason = Some("max_runtime");
                break;
            }

            let status = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::GET,
                "/status",
                None,
            )
            .await?;
            let current_temp_c = require_status_f64(&status, "currentTempC")?;
            let voltage_mv = require_status_u64(&status, "voltageMv")? as f64;
            let current_ma = require_status_u64(&status, "currentMa")? as f64;
            let heater_output_percent = require_status_u64(&status, "heaterOutputPercent")? as f64;
            let board_temp_centi = require_status_i32(&status, "boardTempCenti")? as f64;
            let rtd_raw_adc_mv = require_status_u16(&status, "rtdRawAdcMv")? as f64;
            let vin_raw_adc_mv = require_status_u16(&status, "vinRawAdcMv")? as f64;

            observe_series(&mut current_temp_stats, current_temp_c);
            observe_series(&mut voltage_stats, voltage_mv);
            observe_series(&mut current_ma_stats, current_ma);
            observe_series(&mut heater_output_stats, heater_output_percent);
            observe_series(&mut board_temp_stats, board_temp_centi);
            observe_series(&mut rtd_raw_stats, rtd_raw_adc_mv);
            observe_series(&mut vin_raw_stats, vin_raw_adc_mv);

            let phase = if args.dry_run {
                "dry_run"
            } else {
                "warmup"
            };
            let status_snapshot = status_snapshot(&status)?;
            if first_status_snapshot.is_none() {
                first_status_snapshot = Some(status_snapshot.clone());
            }
            last_status_snapshot = Some(status_snapshot.clone());
            let captured_at_unix_ms = current_unix_millis();
            let elapsed_ms = captured_at_unix_ms.saturating_sub(run_started_unix_ms);
            let sample = json!({
                "runId": run_id.clone(),
                "sampleIndex": sample_index,
                "capturedAtUnixMs": captured_at_unix_ms,
                "elapsedMs": elapsed_ms,
                "phase": phase,
                "sourceCurrentMa": source_current_ma,
                "status": status,
            });
            writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
            samples_writer.flush()?;
            samples_count += 1;

            if !args.dry_run && current_temp_c >= f64::from(args.stop_temp_c) {
                stop_reason = Some("temperature_threshold");
                threshold_sample_index = Some(sample_index);
                break;
            }

            sample_index = sample_index.saturating_add(1);
            let target_tick = next_tick + sample_interval;
            next_tick = target_tick;
            tokio::time::sleep_until(target_tick).await;
        }

        if !args.dry_run {
            let _ = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::PUT,
                "/runtime",
                Some(thermal_self_test_runtime_body(false, args.target_temp_c)),
            )
            .await?;
            heater_stopped = true;
            let stop_status = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::GET,
                "/status",
                None,
            )
            .await?;
            let stop_snapshot = status_snapshot(&stop_status)?;
            let captured_at_unix_ms = current_unix_millis();
            let elapsed_ms = captured_at_unix_ms.saturating_sub(run_started_unix_ms);
            let sample = json!({
                "runId": run_id.clone(),
                "sampleIndex": sample_index.saturating_add(1),
                "capturedAtUnixMs": captured_at_unix_ms,
                "elapsedMs": elapsed_ms,
                "phase": "stopped",
                "sourceCurrentMa": source_current_ma,
                "status": stop_status,
            });
            writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
            samples_writer.flush()?;
            samples_count += 1;
            stopped_sample_index = Some(sample_index.saturating_add(1));
            final_status_snapshot = Some(stop_snapshot.clone());
            last_status_snapshot = Some(stop_snapshot);
        } else {
            final_status_snapshot = last_status_snapshot.clone();
            stop_reason = Some("max_runtime");
        }

        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;

    if heater_started && !heater_stopped {
        let _ = request_leased(
            client,
            &resolved,
            &lease.lease_id,
            Method::PUT,
            "/runtime",
            Some(thermal_self_test_runtime_body(false, args.target_temp_c)),
        )
        .await;
    }

    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();

    collect_result?;

    let duration_ms = current_unix_millis().saturating_sub(run_started_unix_ms);
    let summary = json!({
        "ok": true,
        "runId": run_id.clone(),
        "dryRun": args.dry_run,
        "target": {
            "deviceId": resolved.device.clone(),
            "hardwareId": resolved.hardware_id.clone(),
            "devd": resolved.devd.clone(),
        },
        "source": {
            "deviceId": args.source_device_id,
            "mode": "manual_cc",
            "currentMa": source_current_ma,
        },
        "parameters": {
            "targetTempC": args.target_temp_c,
            "stopTempC": args.stop_temp_c,
            "sampleIntervalMs": args.sample_interval_ms.max(1),
            "maxRuntimeSeconds": args.max_runtime_seconds.max(1),
        },
        "files": {
            "runDir": run_dir,
            "summaryPath": summary_path,
            "samplesPath": samples_path,
        },
        "sampleCount": samples_count,
        "durationMs": duration_ms,
        "stopReason": stop_reason.unwrap_or("max_runtime"),
        "complete": args.dry_run || stop_reason == Some("temperature_threshold"),
        "thresholdSampleIndex": threshold_sample_index,
        "stoppedSampleIndex": stopped_sample_index,
        "startStatus": first_status_snapshot,
        "finalStatus": final_status_snapshot,
        "stats": {
            "currentTempC": current_temp_stats.map(|stats| stats.to_value()),
            "voltageMv": voltage_stats.map(|stats| stats.to_value()),
            "currentMa": current_ma_stats.map(|stats| stats.to_value()),
            "heaterOutputPercent": heater_output_stats.map(|stats| stats.to_value()),
            "boardTempCenti": board_temp_stats.map(|stats| stats.to_value()),
            "rtdRawAdcMv": rtd_raw_stats.map(|stats| stats.to_value()),
            "vinRawAdcMv": vin_raw_stats.map(|stats| stats.to_value()),
        }
    });

    fs::write(&summary_path, serde_json::to_vec_pretty(&summary)?)?;
    if let Some(id) = resolved.hardware_id.as_deref() {
        let _ = remember_usb(id, &resolved.device, &resolved.devd);
    }
    Ok(summary)
}

async fn create_lease(
    client: &Client,
    resolved: &ResolvedUsbTarget,
) -> Result<Lease, Box<dyn std::error::Error + Send + Sync>> {
    let path = format!(
        "/api/v1/devices/{}/leases",
        encode_path_segment(&resolved.device)
    );
    let url = api_url(&resolved.devd, &path)?;
    let mut last_device_not_found = None::<String>;
    for _attempt in 0..20 {
        let response = client.post(url.clone()).send().await?;
        if response.status().is_success() {
            return Ok(response.json::<Lease>().await?);
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.as_u16() == 404 && body.contains("device_not_found") {
            last_device_not_found = Some(body);
            tokio::time::sleep(Duration::from_millis(250)).await;
            continue;
        }
        return Err(format!(
            "create lease failed for {}: HTTP {status} body={body}",
            resolved.device
        )
        .into());
    }
    Err(format!(
        "create lease failed for {} after waiting for native device refresh: {}",
        resolved.device,
        last_device_not_found.unwrap_or_else(|| "device_not_found".to_string())
    )
    .into())
}

async fn create_ready_thermal_lease(
    client: &Client,
    resolved: &ResolvedUsbTarget,
) -> Result<(Lease, Value), Box<dyn std::error::Error + Send + Sync>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut last_error = "device did not become ready".to_string();
    while tokio::time::Instant::now() < deadline {
        match create_lease(client, resolved).await {
            Ok(lease) => {
                match request_leased(
                    client,
                    resolved,
                    &lease.lease_id,
                    Method::GET,
                    "/status",
                    None,
                )
                .await
                {
                    Ok(mut status) => {
                        if status.get("heaterEnabled").and_then(Value::as_bool) == Some(true) {
                            force_thermal_self_test_shutdown(client, resolved, &lease.lease_id)
                                .await?;
                            status = request_leased(
                                client,
                                resolved,
                                &lease.lease_id,
                                Method::GET,
                                "/status",
                                None,
                            )
                            .await?;
                        }
                        if status.get("heaterEnabled").and_then(Value::as_bool) == Some(false) {
                            return Ok((lease, status));
                        }
                        last_error = "thermal readiness status did not confirm heater off".into();
                    }
                    Err(error) => {
                        last_error = error.to_string();
                    }
                }
                let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
            }
            Err(error) => {
                last_error = error.to_string();
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(format!(
        "thermal device readiness handshake timed out for {}: {last_error}",
        resolved.device
    )
    .into())
}

async fn release_lease(
    client: &Client,
    devd: &str,
    lease_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _ = client
        .delete(api_url(devd, &format!("/api/v1/leases/{lease_id}"))?)
        .send()
        .await?;
    Ok(())
}

async fn force_thermal_self_test_shutdown(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cleanup_body = thermal_self_test_cooldown_runtime_body();
    if request_leased(
        client,
        resolved,
        lease_id,
        Method::PUT,
        "/runtime",
        Some(cleanup_body.clone()),
    )
    .await
    .is_ok()
    {
        return Ok(());
    }

    let _ = release_lease(client, &resolved.devd, lease_id).await;
    let recovery_lease = create_lease(client, resolved).await?;
    let recovery_heartbeat = spawn_heartbeat(
        client.clone(),
        resolved.devd.clone(),
        recovery_lease.clone(),
    );
    let result = request_leased(
        client,
        resolved,
        &recovery_lease.lease_id,
        Method::PUT,
        "/runtime",
        Some(cleanup_body),
    )
    .await;
    let _ = release_lease(client, &resolved.devd, &recovery_lease.lease_id).await;
    recovery_heartbeat.abort();
    result.map(|_| ())
}

fn spawn_heartbeat(client: Client, devd: String, lease: Lease) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval_ms = (lease.ttl_ms / 2).max(500);
        let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
        loop {
            interval.tick().await;
            let Ok(url) = api_url(
                &devd,
                &format!("/api/v1/leases/{}/heartbeat", lease.lease_id),
            ) else {
                break;
            };
            if client.post(url).send().await.is_err() {
                break;
            }
        }
    })
}

async fn runtime_body(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    args: RuntimeSetArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut body = serde_json::Map::new();
    insert_if_some(&mut body, "targetTempC", args.target_temp_c);
    insert_if_some(&mut body, "selectedPresetSlot", args.selected_preset_slot);
    insert_if_some(&mut body, "activeCoolingEnabled", args.active_cooling);
    insert_if_some(&mut body, "heaterEnabled", args.heater_enabled);
    if args.fault_attention_acknowledged {
        body.insert("faultAttentionAcknowledged".to_string(), json!(true));
    }
    if let Some(file) = args.presets_file {
        body.insert("presetsC".to_string(), read_json_file(&file)?);
    }
    if args.preset_slot.is_some() || args.preset_temp_c.is_some() || args.preset_disabled {
        let slot = args
            .preset_slot
            .ok_or("preset edit requires --preset-slot")?;
        let status =
            request_with_lease(client, resolved.clone(), Method::GET, "/status", None).await?;
        let mut presets = status
            .get("presetsC")
            .and_then(Value::as_array)
            .cloned()
            .ok_or("status did not include presetsC")?;
        if slot >= presets.len() {
            return Err("preset slot is out of range".into());
        }
        presets[slot] = if args.preset_disabled {
            Value::Null
        } else {
            json!(
                args.preset_temp_c
                    .ok_or("preset edit requires --preset-temp-c or --preset-disabled")?
            )
        };
        body.insert("presetsC".to_string(), Value::Array(presets));
    }
    if body.is_empty() {
        return Err("runtime set requires at least one field".into());
    }
    Ok(Value::Object(body))
}

fn parse_pps_volts(value: &str) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err("PPS voltage must be a positive decimal value".into());
    }

    let (whole, fractional) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.len() > 1
    {
        return Err("PPS voltage must use at most one decimal place".into());
    }

    let whole_mv: u32 = whole.parse::<u32>()?.saturating_mul(1_000);
    let fractional_mv = if fractional.is_empty() {
        0
    } else {
        u32::from(fractional.as_bytes()[0] - b'0') * 100
    };
    let millivolts = whole_mv.saturating_add(fractional_mv);
    if !(5_000..=28_000).contains(&millivolts) {
        return Err("PPS voltage must stay within the hardware 5.0V to 28.0V range".into());
    }

    Ok(millivolts as u16)
}

fn parse_voltage_to_mv(value: &str) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err("voltage must be a positive decimal value".into());
    }

    let (whole, fractional) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.len() > 3
    {
        return Err("voltage must use at most three decimal places".into());
    }

    let whole_mv = whole.parse::<u32>()?.saturating_mul(1_000);
    let fractional_mv = match fractional.len() {
        0 => 0,
        1 => u32::from(fractional.as_bytes()[0] - b'0') * 100,
        2 => {
            u32::from(fractional.as_bytes()[0] - b'0') * 100
                + u32::from(fractional.as_bytes()[1] - b'0') * 10
        }
        _ => {
            u32::from(fractional.as_bytes()[0] - b'0') * 100
                + u32::from(fractional.as_bytes()[1] - b'0') * 10
                + u32::from(fractional.as_bytes()[2] - b'0')
        }
    };
    Ok(whole_mv.saturating_add(fractional_mv))
}

fn parse_pps_amps(value: &str) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err("PPS current must be a positive decimal value".into());
    }

    let (whole, fractional) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.len() > 2
    {
        return Err("PPS current must use at most two decimal places".into());
    }

    let whole_ma: u32 = whole.parse::<u32>()?.saturating_mul(1_000);
    let fractional_ma = match fractional.len() {
        0 => 0,
        1 => u32::from(fractional.as_bytes()[0] - b'0') * 100,
        _ => {
            u32::from(fractional.as_bytes()[0] - b'0') * 100
                + u32::from(fractional.as_bytes()[1] - b'0') * 10
        }
    };
    let milliamps = whole_ma.saturating_add(fractional_ma);
    if milliamps == 0 || milliamps > u32::from(u16::MAX) || !milliamps.is_multiple_of(50) {
        return Err("PPS current must be greater than 0A and use 0.05A steps".into());
    }

    Ok(milliamps as u16)
}

fn parse_thermal_targets(
    value: Option<&str>,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    let Some(value) = value else {
        return Ok(THERMAL_SELF_TEST_DEFAULT_TARGETS_C.to_vec());
    };
    let mut targets = Vec::new();
    for raw in value.split(',') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("thermal targets must not contain empty items".into());
        }
        let target = trimmed.parse::<i16>()?;
        if !THERMAL_SUPPORTED_TARGETS_C.contains(&target) {
            return Err(format!(
                "thermal target {target}C is unsupported; allowed: 60,80,100,120,140,160,180,200,220,240,250"
            )
            .into());
        }
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    if targets.is_empty() {
        return Err("thermal targets must not be empty".into());
    }
    targets.sort_unstable_by_key(|target| {
        THERMAL_SUPPORTED_TARGETS_C
            .iter()
            .position(|candidate| candidate == target)
            .unwrap_or(usize::MAX)
    });
    Ok(targets)
}

fn insert_if_some<T: Serialize>(
    body: &mut serde_json::Map<String, Value>,
    key: &str,
    value: Option<T>,
) {
    if let Some(value) = value {
        body.insert(key.to_string(), json!(value));
    }
}

async fn resolve_artifact(
    client: &Client,
    devd: &str,
    manifest_path: Option<&Path>,
    artifact_id: Option<&str>,
) -> Result<FirmwareArtifact, Box<dyn std::error::Error + Send + Sync>> {
    let artifacts = if let Some(manifest_path) = manifest_path {
        read_artifact_manifest(manifest_path)?
    } else {
        let payload = request_json(client, Method::GET, devd, "/api/v1/artifacts", None).await?;
        serde_json::from_value::<FirmwareArtifactCatalog>(payload)?.artifacts
    };
    if let Some(artifact_id) = artifact_id {
        return artifacts
            .into_iter()
            .find(|artifact| artifact.artifact_id == artifact_id)
            .ok_or_else(|| format!("artifact not found: {artifact_id}").into());
    }
    match artifacts.as_slice() {
        [artifact] => Ok(artifact.clone()),
        [] => Err("no firmware artifacts found".into()),
        _ => Err("multiple artifacts found; pass --artifact-id".into()),
    }
}

fn read_artifact_manifest(
    path: &Path,
) -> Result<Vec<FirmwareArtifact>, Box<dyn std::error::Error + Send + Sync>> {
    let value: Value = serde_json::from_slice(&fs::read(path)?)?;
    if let Ok(catalog) = serde_json::from_value::<FirmwareArtifactCatalog>(value.clone()) {
        return Ok(catalog.artifacts);
    }
    if let Ok(artifact) = serde_json::from_value::<FirmwareArtifact>(value.clone()) {
        return Ok(vec![artifact]);
    }
    serde_json::from_value::<Vec<FirmwareArtifact>>(value).map_err(Into::into)
}

async fn monitor_once(
    client: &Client,
    resolved: ResolvedUsbTarget,
    tail: usize,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let lease = create_lease(client, &resolved).await?;
    let devices_result =
        request_json(client, Method::GET, &resolved.devd, "/api/v1/devices", None).await;
    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    let devices = devices_result?;
    let events = devices
        .get("devices")
        .and_then(Value::as_array)
        .and_then(|devices| {
            devices
                .iter()
                .find(|device| device.get("id").and_then(Value::as_str) == Some(&resolved.device))
        })
        .and_then(|device| device.get("events"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let start = events.len().saturating_sub(tail);
    Ok(json!({"device": resolved.device, "events": &events[start..]}))
}

async fn handle_hardware_command(
    client: &Client,
    default_devd: &str,
    command: HardwareCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        HardwareCommand::Available => {
            let registry = read_hardware_registry()?;
            let devd_devices =
                request_json(client, Method::GET, default_devd, "/api/v1/devices", None)
                    .await
                    .unwrap_or_else(|error| json!({"error": error.to_string()}));
            Ok(json!({
                "path": path_string(hardware_registry_path()?),
                "devd": default_devd,
                "usb": {
                    "devices": devd_devices,
                    "remembered": registry.hardware,
                }
            }))
        }
        HardwareCommand::Recent => {
            let mut registry = read_hardware_registry()?;
            registry.hardware.sort_by_key(|hardware| {
                std::cmp::Reverse(hardware.last_seen_unix_seconds.unwrap_or(0))
            });
            Ok(
                json!({"path": path_string(hardware_registry_path()?), "hardware": registry.hardware}),
            )
        }
        HardwareCommand::List => {
            let registry = read_hardware_registry()?;
            Ok(
                json!({"path": path_string(hardware_registry_path()?), "hardware": registry.hardware}),
            )
        }
        HardwareCommand::Path => Ok(json!({"path": path_string(hardware_registry_path()?)})),
        HardwareCommand::Save {
            id,
            name,
            device,
            devd,
        } => {
            let mut registry = read_hardware_registry()?;
            let hardware = SavedHardware {
                id,
                name,
                transport: SavedTransport::Usb,
                device,
                devd: devd.or_else(|| Some(default_devd.to_string())),
                last_seen_unix_seconds: Some(current_unix_seconds()),
            };
            let saved = upsert_hardware(&mut registry, hardware);
            write_hardware_registry(&registry)?;
            Ok(json!({"path": path_string(hardware_registry_path()?), "hardware": saved}))
        }
        HardwareCommand::Forget { id } => {
            let mut registry = read_hardware_registry()?;
            let before = registry.hardware.len();
            registry.hardware.retain(|hardware| hardware.id != id);
            write_hardware_registry(&registry)?;
            Ok(
                json!({"path": path_string(hardware_registry_path()?), "id": id, "removed": registry.hardware.len() != before}),
            )
        }
    }
}

fn handle_usb_port_command(
    command: UsbPortCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        UsbPortCommand::Set { port } => {
            let mut config = read_user_config().unwrap_or_default();
            config.default_serial_port = Some(port.clone());
            write_user_config(&config)?;
            Ok(
                json!({"ok": true, "defaultSerialPort": port, "configPath": path_string(flux_purr_devd::user_config_path()?)}),
            )
        }
        UsbPortCommand::Show => {
            let config = read_user_config().unwrap_or_default();
            Ok(
                json!({"configPath": path_string(flux_purr_devd::user_config_path()?), "defaultSerialPort": config.default_serial_port}),
            )
        }
    }
}

fn resolve_target(
    selector: TargetSelector,
    default_devd: &str,
) -> Result<ResolvedUsbTarget, Box<dyn std::error::Error + Send + Sync>> {
    match (selector.device, selector.hardware) {
        (Some(_), Some(_)) => Err("command accepts only one of --device or --hardware".into()),
        (Some(device), None) => Ok(ResolvedUsbTarget {
            device,
            devd: default_devd.to_string(),
            hardware_id: None,
        }),
        (None, Some(id)) => {
            let registry = read_hardware_registry()?;
            let hardware = registry
                .hardware
                .iter()
                .find(|hardware| hardware.id == id)
                .ok_or_else(|| format!("saved hardware not found: {id}"))?;
            Ok(ResolvedUsbTarget {
                device: hardware.device.clone(),
                devd: hardware
                    .devd
                    .clone()
                    .unwrap_or_else(|| default_devd.to_string()),
                hardware_id: Some(id),
            })
        }
        (None, None) => Err("command requires --device or --hardware".into()),
    }
}

fn api_url(base: &str, path: &str) -> Result<Url, Box<dyn std::error::Error + Send + Sync>> {
    let mut url = Url::parse(base)?;
    url.set_path(path);
    Ok(url)
}

fn encode_path_segment(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn read_json_file(path: &Path) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn path_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

fn read_hardware_registry() -> io::Result<HardwareRegistry> {
    let path = hardware_registry_path()?;
    if !path.exists() {
        return Ok(HardwareRegistry::default());
    }
    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(HardwareRegistry::default());
    }
    serde_json::from_str(&content)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn write_hardware_registry(registry: &HardwareRegistry) -> io::Result<()> {
    let path = hardware_registry_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(registry)?)
}

fn remember_usb(id: &str, device: &str, devd: &str) -> io::Result<()> {
    let mut registry = read_hardware_registry()?;
    upsert_hardware(
        &mut registry,
        SavedHardware {
            id: id.to_string(),
            name: None,
            transport: SavedTransport::Usb,
            device: device.to_string(),
            devd: Some(devd.to_string()),
            last_seen_unix_seconds: Some(current_unix_seconds()),
        },
    );
    write_hardware_registry(&registry)
}

fn upsert_hardware(registry: &mut HardwareRegistry, mut hardware: SavedHardware) -> SavedHardware {
    if let Some(existing) = registry
        .hardware
        .iter_mut()
        .find(|existing| existing.id == hardware.id)
    {
        if hardware.name.is_none() {
            hardware.name = existing.name.clone();
        }
        *existing = hardware.clone();
    } else {
        registry.hardware.push(hardware.clone());
    }
    registry
        .hardware
        .sort_by(|left, right| left.id.cmp(&right.id));
    hardware
}

fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn hardware_registry_schema_version() -> u8 {
    1
}

fn redact_cli_sensitive(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let key_lc = key.to_ascii_lowercase();
                    if matches!(
                        key_lc.as_str(),
                        "password" | "psk" | "passphrase" | "secret" | "token"
                    ) {
                        (key.clone(), Value::String("<redacted>".to_string()))
                    } else {
                        (key.clone(), redact_cli_sensitive(value))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_cli_sensitive).collect()),
        _ => value.clone(),
    }
}

fn render_human(payload: &Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(devices) = payload.get("devices").and_then(Value::as_array) {
        return Ok(format!("Devices: {}", devices.len()));
    }
    if let Some(device) = payload.get("deviceId").and_then(Value::as_str) {
        return Ok(format!(
            "{} target={}C current={}C heater={} cooling={}",
            device,
            payload
                .get("targetTempC")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            payload
                .get("currentTempC")
                .and_then(Value::as_f64)
                .unwrap_or_default(),
            payload
                .get("heaterEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            payload
                .get("activeCoolingEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ));
    }
    if payload.get("artifactId").is_some() && payload.get("status").is_some() {
        return Ok(format!(
            "Flash {}: {}",
            payload
                .get("artifactId")
                .and_then(Value::as_str)
                .unwrap_or("-"),
            payload.get("status").and_then(Value::as_str).unwrap_or("-")
        ));
    }
    if payload.get("rtdAdc").is_some() && payload.get("vinAdc").is_some() {
        let rtd_count = payload
            .get("rtdAdc")
            .and_then(|channel| channel.get("samples"))
            .and_then(Value::as_array)
            .map(|items| items.iter().filter(|item| !item.is_null()).count())
            .unwrap_or(0);
        let vin_count = payload
            .get("vinAdc")
            .and_then(|channel| channel.get("samples"))
            .and_then(Value::as_array)
            .map(|items| items.iter().filter(|item| !item.is_null()).count())
            .unwrap_or(0);
        return Ok(format!(
            "Calibration: rtd_adc={} samples vin_adc={} samples",
            rtd_count, vin_count
        ));
    }
    if payload.get("kind").and_then(Value::as_str) == Some("thermal_self_test") {
        return Ok(format!(
            "Thermal self-test {}: {} samples passed={}",
            payload.get("runId").and_then(Value::as_str).unwrap_or("-"),
            payload
                .get("sampleCount")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            payload
                .get("validation")
                .and_then(|validation| validation.get("passed"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ));
    }
    if payload.get("kind").and_then(Value::as_str) == Some("thermal_self_test_replay") {
        return Ok(format!(
            "Thermal replay {}: {} samples passed={}",
            payload.get("runId").and_then(Value::as_str).unwrap_or("-"),
            payload
                .get("sampleCount")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            payload
                .get("validation")
                .and_then(|validation| validation.get("passed"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ));
    }
    if payload.get("operation").and_then(Value::as_str)
        == Some("thermal_report.rerender_legacy_preliminary_review_bundle")
    {
        return Ok(format!(
            "Thermal report bundle: {}",
            payload
                .get("bundleIndexHtml")
                .and_then(Value::as_str)
                .unwrap_or("-")
        ));
    }
    if payload.get("runId").is_some() && payload.get("sampleCount").is_some() {
        return Ok(format!(
            "Calibration run {}: {} samples stop={} complete={}",
            payload.get("runId").and_then(Value::as_str).unwrap_or("-"),
            payload
                .get("sampleCount")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            payload
                .get("stopReason")
                .and_then(Value::as_str)
                .unwrap_or("-"),
            payload
                .get("complete")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        ));
    }
    if payload.get("hardware").is_some() || payload.get("usb").is_some() {
        return Ok(serde_json::to_string_pretty(&redact_cli_sensitive(
            payload,
        ))?);
    }
    if payload.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok("OK".to_string());
    }
    Ok(serde_json::to_string_pretty(&redact_cli_sensitive(
        payload,
    ))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{
        Json, Router,
        extract::{Path as AxumPath, State},
        http::StatusCode,
        routing::{delete, get, post, put},
    };

    #[test]
    fn encodes_device_id_as_single_path_segment() {
        assert_eq!(
            encode_path_segment("serial-303a-1001-D0:CF"),
            "serial-303a-1001-D0%3ACF"
        );
    }

    #[test]
    fn output_enable_requires_explicit_target_selector() {
        let err = resolve_target(
            TargetSelector {
                device: Some("a".to_string()),
                hardware: Some("b".to_string()),
            },
            DEFAULT_DEVD_URL,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("only one"));
    }

    #[test]
    fn hardware_upsert_preserves_existing_name_when_unspecified() {
        let mut registry = HardwareRegistry::default();
        upsert_hardware(
            &mut registry,
            SavedHardware {
                id: "bench".to_string(),
                name: Some("Bench".to_string()),
                transport: SavedTransport::Usb,
                device: "dev-1".to_string(),
                devd: Some(DEFAULT_DEVD_URL.to_string()),
                last_seen_unix_seconds: Some(1),
            },
        );
        let updated = upsert_hardware(
            &mut registry,
            SavedHardware {
                id: "bench".to_string(),
                name: None,
                transport: SavedTransport::Usb,
                device: "dev-2".to_string(),
                devd: Some(DEFAULT_DEVD_URL.to_string()),
                last_seen_unix_seconds: Some(2),
            },
        );
        assert_eq!(updated.name.as_deref(), Some("Bench"));
        assert_eq!(registry.hardware[0].device, "dev-2");
    }

    #[test]
    fn redacts_nested_cli_secrets() {
        let payload = json!({"wifi": {"password": "secret"}, "token": "abc"});
        let redacted = redact_cli_sensitive(&payload);
        assert_eq!(redacted["wifi"]["password"], "<redacted>");
        assert_eq!(redacted["token"], "<redacted>");
    }

    #[test]
    fn renders_calibration_collect_summary() {
        let payload = json!({
            "runId": "cal-1",
            "sampleCount": 42,
            "stopReason": "temperature_threshold",
            "complete": true,
        });
        let rendered = render_human(&payload).unwrap();
        assert!(rendered.contains(
            "Calibration run cal-1: 42 samples stop=temperature_threshold complete=true"
        ));
    }

    #[test]
    fn renders_thermal_self_test_summary() {
        let payload = json!({
            "kind": "thermal_self_test",
            "runId": "thermal-1",
            "sampleCount": 36,
            "validation": { "passed": true },
        });
        let rendered = render_human(&payload).unwrap();
        assert!(rendered.contains("Thermal self-test thermal-1: 36 samples passed=true"));
    }

    #[test]
    fn renders_thermal_replay_summary() {
        let payload = json!({
            "kind": "thermal_self_test_replay",
            "runId": "thermal-1",
            "sampleCount": 36,
            "validation": { "passed": false },
        });
        let rendered = render_human(&payload).unwrap();
        assert!(rendered.contains("Thermal replay thermal-1: 36 samples passed=false"));
    }

    #[test]
    fn thermal_candidate_profile_excludes_300c() {
        let profile = default_thermal_candidate_profile();
        let targets = profile["points"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|point| point.get("targetTempC").and_then(Value::as_i64))
            .collect::<Vec<_>>();
        assert_eq!(targets, vec![60, 100, 140, 180, 220, 250]);
        assert_eq!(profile["points"].as_array().unwrap().len(), 10);
    }

    #[test]
    fn parse_thermal_targets_defaults_and_sorts_subset() {
        assert_eq!(
            parse_thermal_targets(None).unwrap(),
            THERMAL_SELF_TEST_DEFAULT_TARGETS_C.to_vec()
        );
        assert_eq!(
            parse_thermal_targets(Some("250,140,250")).unwrap(),
            vec![140, 250]
        );
        assert_eq!(
            parse_thermal_targets(Some("220,100,220")).unwrap(),
            vec![100, 220]
        );
        assert_eq!(
            parse_thermal_targets(Some("240,80,120,80")).unwrap(),
            vec![80, 120, 240]
        );
    }

    #[test]
    fn resolve_optimization_targets_prefers_sparse_range_covering_subset() {
        assert_eq!(
            resolve_optimization_targets(&[60, 100, 140, 180, 220, 250], None).unwrap(),
            vec![60, 140, 250]
        );
        assert_eq!(
            resolve_optimization_targets(&[140, 250], None).unwrap(),
            vec![140, 250]
        );
    }

    #[test]
    fn parse_thermal_targets_rejects_unsupported_values() {
        let error = parse_thermal_targets(Some("60,300")).unwrap_err();
        assert!(error.to_string().contains("unsupported"));
    }

    #[test]
    fn thermal_heater_parameters_interpolate_between_profile_anchors() {
        let mut profile = thermal_seed_candidate_profile();
        let lower = thermal_candidate_point(&profile, 60).expect("60C anchor");
        let upper = thermal_candidate_point(&profile, 100).expect("100C anchor");
        thermal_candidate_point_mut(&mut profile, 60)
            .expect("60C anchor")
            .brake_distance_centi_c = 800;
        thermal_candidate_point_mut(&mut profile, 100)
            .expect("100C anchor")
            .brake_distance_centi_c = 1_100;
        thermal_candidate_point_mut(&mut profile, 60)
            .expect("60C anchor")
            .approach_tail_window_centi_c = 120;
        thermal_candidate_point_mut(&mut profile, 100)
            .expect("100C anchor")
            .approach_tail_window_centi_c = 280;
        let point = thermal_interpolated_candidate_point(&profile, 80).expect("80C point");
        assert_eq!(point.brake_distance_centi_c, 1_140);
        assert_eq!(point.approach_tail_window_centi_c, 200);
        assert_eq!(
            point.approach_power_permille,
            (lower.approach_power_permille + upper.approach_power_permille + 1) / 2
        );

        let value = thermal_candidate_profile_to_value(&profile);
        let parameters = thermal_heater_parameters_value(80, Some(&value), "preview");
        assert_eq!(parameters["targetTempC"], 80);
        assert_eq!(parameters["brakeDistanceCentiC"], 1_140);
        assert_eq!(parameters["approachTailWindowCentiC"], 200);
        assert_eq!(
            parameters["approachPowerPermille"],
            point.approach_power_permille
        );
    }

    #[test]
    fn thermal_heater_parameters_apply_firmware_zero_value_inheritance() {
        let mut profile = thermal_seed_candidate_profile();
        profile.settings.hold_ki_permille_per_c_tick = 1;
        thermal_candidate_point_mut(&mut profile, 60)
            .expect("60C anchor")
            .hold_ki_permille_per_c_tick = 0;

        let value = thermal_candidate_profile_to_value(&profile);
        let parameters = thermal_heater_parameters_value(60, Some(&value), "preview");
        assert_eq!(parameters["holdKiPermillePerCTick"], 1);
    }

    #[test]
    fn thermal_profile_preview_unwraps_runtime_wrapper() {
        let profile = default_thermal_candidate_profile();
        let imported = json!({
            "thermalControlProfile": {
                "op": "preview",
                "profile": profile.clone(),
            }
        });

        assert_eq!(thermal_profile_package_from_value(imported), profile);
    }

    #[test]
    fn thermal_control_readback_rejects_any_effective_parameter_mismatch() {
        let profile = default_thermal_candidate_profile();
        let expected = thermal_heater_parameters_value(60, Some(&profile), "preview");
        let mut actual = expected.as_object().unwrap().clone();
        let settings = actual.remove("settings").unwrap();
        actual.remove("mode");
        actual.insert("profileActive".to_string(), Value::Bool(true));
        actual.insert("profileCoversTarget".to_string(), Value::Bool(true));
        actual.insert(
            "profileSource".to_string(),
            Value::String("preview".to_string()),
        );
        for field in [
            "tempFilterAlphaPermille",
            "warmupReenterCentiC",
            "approachMaxTicks",
            "approachMinPowerRatioPermille",
            "autoAdjustableWorkingFloorMv",
            "heaterCurrentReserveMa",
        ] {
            actual.insert(field.to_string(), settings[field].clone());
        }
        let status = json!({
            "thermalControlProfilePreview": true,
            "thermalControl": Value::Object(actual.clone()),
        });
        assert!(verify_thermal_control_readback(&status, &expected, "preview").is_ok());

        actual.insert("warmupPowerPermille".to_string(), json!(999));
        let mismatch = json!({
            "thermalControlProfilePreview": true,
            "thermalControl": Value::Object(actual),
        });
        assert!(
            verify_thermal_control_readback(&mismatch, &expected, "preview")
                .unwrap_err()
                .to_string()
                .contains("warmupPowerPermille")
        );
    }

    #[test]
    fn thermal_candidate_profile_parses_existing_profile_value() {
        let profile = default_thermal_candidate_profile();
        let parsed = thermal_candidate_profile_from_value(profile.clone());
        assert_eq!(thermal_candidate_profile_to_value(&parsed), profile);
    }

    #[test]
    fn thermal_candidate_profile_forces_full_power_warmup() {
        let mut profile = default_thermal_candidate_profile();
        profile["points"][0]["warmupPowerPermille"] = json!(760);

        let parsed = thermal_candidate_profile_from_value(profile);
        let normalized = thermal_candidate_profile_to_value(&parsed);
        assert_eq!(normalized["points"][0]["warmupPowerPermille"], 1_000);

        let point = thermal_candidate_point(&parsed, 60).expect("60C point");
        let effective = thermal_effective_candidate_point(point, &parsed.settings);
        assert_eq!(effective.warmup_power_permille, 1_000);

        let params = thermal_heater_parameters_value(60, Some(&normalized), "preview");
        assert_eq!(params["warmupPowerPermille"], 1_000);
    }

    #[test]
    fn thermal_candidate_profile_keeps_full_power_warmup_after_interpolation() {
        let mut profile = default_thermal_candidate_profile();
        profile["points"][0]["targetTempC"] = json!(60);
        profile["points"][0]["warmupPowerPermille"] = json!(400);
        profile["points"][0]["approachPowerPermille"] = json!(440);
        profile["points"][1]["targetTempC"] = json!(100);
        profile["points"][1]["warmupPowerPermille"] = json!(1000);
        profile["points"][1]["approachPowerPermille"] = json!(445);

        let parsed = thermal_candidate_profile_from_value(profile);
        assert_eq!(parsed.points[0].warmup_power_permille, 1_000);

        let interpolated =
            thermal_interpolated_candidate_point(&parsed, 80).expect("80C interpolated point");
        assert_eq!(interpolated.warmup_power_permille, 1_000);

        let params = thermal_heater_parameters_value(
            80,
            Some(&thermal_candidate_profile_to_value(&parsed)),
            "preview",
        );
        assert_eq!(params["warmupPowerPermille"], 1_000);
    }

    #[test]
    fn thermal_candidate_profile_preserves_explicit_calibration_target() {
        let mut profile = default_thermal_candidate_profile();
        profile["points"][5]["targetTempC"] = json!(80);
        profile["points"][5]["brakeDistanceCentiC"] = json!(450);

        let normalized =
            thermal_candidate_profile_to_value(&thermal_candidate_profile_from_value(profile));
        assert_eq!(normalized["points"][5]["targetTempC"], 80);
        assert_eq!(normalized["points"][5]["brakeDistanceCentiC"], 450);
    }

    #[test]
    fn thermal_candidate_profile_normalizes_missing_current_reserve() {
        let mut profile = default_thermal_candidate_profile();
        profile["settings"]
            .as_object_mut()
            .unwrap()
            .remove("heaterCurrentReserveMa");

        let normalized =
            thermal_candidate_profile_to_value(&thermal_candidate_profile_from_value(profile));
        assert_eq!(normalized["settings"]["heaterCurrentReserveMa"], 200);
    }

    #[test]
    fn thermal_validation_rejects_incomplete_stages() {
        let applied = vec![ThermalStageResult {
            target_temp_c: 120,
            rise_time_ms: 119_000,
            max_overshoot_c: 1.0,
            hold_peak_to_peak_c: 1.0,
            sample_count: 12,
            stop_reason: "timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis::default(),
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        }];

        let validation = validate_thermal_applied_results(
            &applied,
            &[120],
            ThermalSelfTestEvaluationMode::HoldConfirm,
        );
        assert_eq!(validation["passed"], false);
        assert_eq!(validation["failures"][0]["reason"], "incomplete_stage");
    }

    #[test]
    fn thermal_validation_rejects_a_partial_target_ladder() {
        let applied = vec![ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 10_000,
            max_overshoot_c: 1.0,
            hold_peak_to_peak_c: 1.0,
            sample_count: 40,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis::default(),
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                warmup_exited_at_ms: Some(1_000),
                stable_window_started_at_ms: Some(6_000),
                stable_window_verified_at_ms: Some(16_000),
                settle_time_ms: Some(5_000),
                failure_reason: None,
            },
        }];

        let validation = validate_thermal_applied_results(
            &applied,
            &[60, 140],
            ThermalSelfTestEvaluationMode::HoldConfirm,
        );

        assert_eq!(validation["passed"], false);
        assert_eq!(validation["failures"][0]["reason"], "missing_stage");
        assert_eq!(validation["failures"][0]["targetTempC"], 140);
    }

    #[test]
    fn thermal_tuning_scout_validation_ignores_hold_limits() {
        let applied = vec![ThermalStageResult {
            target_temp_c: 140,
            rise_time_ms: 11_500,
            max_overshoot_c: 3.8,
            hold_peak_to_peak_c: 3.6,
            sample_count: 40,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis::default(),
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                warmup_exited_at_ms: Some(1_000),
                stable_window_started_at_ms: None,
                stable_window_verified_at_ms: None,
                settle_time_ms: None,
                failure_reason: Some("full_speed_to_stable_timeout"),
            },
        }];

        let validation = validate_thermal_applied_results(
            &applied,
            &[140],
            ThermalSelfTestEvaluationMode::TuningScout,
        );

        assert_eq!(validation["passed"], true);
        assert_eq!(
            validation["failures"].as_array().map(|items| items.len()),
            Some(0)
        );
    }

    #[test]
    fn thermal_hold_confirm_validation_rejects_slow_full_speed_settle() {
        let applied = vec![ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 18_000,
            max_overshoot_c: 1.2,
            hold_peak_to_peak_c: 1.4,
            sample_count: 90,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis::default(),
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                warmup_exited_at_ms: Some(1_000),
                stable_window_started_at_ms: Some(19_000),
                stable_window_verified_at_ms: Some(29_000),
                settle_time_ms: Some(18_000),
                failure_reason: Some("full_speed_to_stable_timeout"),
            },
        }];

        let validation = validate_thermal_applied_results(
            &applied,
            &[60],
            ThermalSelfTestEvaluationMode::HoldConfirm,
        );

        assert_eq!(validation["passed"], false);
        assert_eq!(validation["failures"][0]["reason"], "full_speed_to_stable");
        assert_eq!(validation["failures"][0]["limit"], 10_000);
    }

    #[test]
    fn thermal_hold_confirm_validation_uses_five_seconds_above_150c() {
        let applied = vec![ThermalStageResult {
            target_temp_c: 180,
            rise_time_ms: 7_000,
            max_overshoot_c: 1.2,
            hold_peak_to_peak_c: 1.4,
            sample_count: 90,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis::default(),
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                warmup_exited_at_ms: Some(1_000),
                stable_window_started_at_ms: Some(7_000),
                stable_window_verified_at_ms: Some(17_000),
                settle_time_ms: Some(6_000),
                failure_reason: Some("full_speed_to_stable_timeout"),
            },
        }];

        let validation = validate_thermal_applied_results(
            &applied,
            &[180],
            ThermalSelfTestEvaluationMode::HoldConfirm,
        );

        assert_eq!(validation["passed"], false);
        assert_eq!(validation["failures"][0]["reason"], "full_speed_to_stable");
        assert_eq!(validation["failures"][0]["limit"], 5_000);
    }

    #[test]
    fn thermal_hold_confirm_validation_allows_ten_seconds_at_or_below_150c() {
        let applied = vec![ThermalStageResult {
            target_temp_c: 150,
            rise_time_ms: 10_000,
            max_overshoot_c: 1.2,
            hold_peak_to_peak_c: 1.4,
            sample_count: 90,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis::default(),
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                warmup_exited_at_ms: Some(1_000),
                stable_window_started_at_ms: Some(11_000),
                stable_window_verified_at_ms: Some(21_000),
                settle_time_ms: Some(10_000),
                failure_reason: None,
            },
        }];

        let validation = validate_thermal_applied_results(
            &applied,
            &[150],
            ThermalSelfTestEvaluationMode::HoldConfirm,
        );

        assert_eq!(validation["passed"], true);
        assert_eq!(
            validation["failures"].as_array().map(|items| items.len()),
            Some(0)
        );
    }

    #[test]
    fn thermal_tuning_continues_only_after_controllable_heat_failures() {
        let mut stage = ThermalStageResult {
            target_temp_c: 140,
            rise_time_ms: 10_500,
            max_overshoot_c: 0.0,
            hold_peak_to_peak_c: f64::INFINITY,
            sample_count: 42,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis::default(),
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        };

        assert!(thermal_stage_can_continue_tuning(&stage));
        stage.stop_reason = "warmup_timeout";
        assert!(thermal_stage_can_continue_tuning(&stage));
        stage.stop_reason = "heater_disarmed";
        assert!(!thermal_stage_can_continue_tuning(&stage));
    }

    #[test]
    fn thermal_infrastructure_failure_does_not_change_candidate() {
        let previous = thermal_default_target_point(60);
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 3_708,
                max_overshoot_c: 0.0,
                hold_peak_to_peak_c: f64::INFINITY,
                sample_count: 15,
                stop_reason: "sample_rate_below_3hz",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis::default(),
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert_eq!(tuned, previous);
    }

    #[test]
    fn thermal_pre_hold_timeout_does_not_raise_hold_power() {
        let previous = thermal_default_target_point(60);
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 17_680,
                max_overshoot_c: 0.0,
                hold_peak_to_peak_c: f64::INFINITY,
                sample_count: 71,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    approach_median_output_permille: Some(340),
                    approach_sample_count: 30,
                    hold_sample_count: 0,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert_eq!(tuned.hold_power_permille, previous.hold_power_permille);
        assert_eq!(
            tuned.hold_reheat_power_permille,
            previous.hold_reheat_power_permille
        );
        assert!(tuned.approach_floor_power_permille >= previous.approach_floor_power_permille);
        assert!(tuned.brake_distance_centi_c < previous.brake_distance_centi_c);
    }

    #[test]
    fn thermal_pre_hold_threshold_crossing_advances_lead_without_raising_power() {
        let mut previous = thermal_default_target_point(60);
        previous.brake_distance_centi_c = 1_310;
        previous.approach_power_permille = 590;
        previous.approach_floor_power_permille = 510;
        previous.approach_lead_ticks = 0;

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 26_132,
                max_overshoot_c: 2.8,
                hold_peak_to_peak_c: f64::INFINITY,
                sample_count: 249,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    approach_median_output_permille: Some(530),
                    approach_median_slope_c_per_s: Some(7.5),
                    approach_sample_count: 61,
                    hold_sample_count: 0,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis {
                    approach_started_at_ms: Some(15_871),
                    hold_threshold_crossed_at_ms: Some(23_736),
                    ..ThermalApproachGuardAnalysis::default()
                },
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    warmup_exited_at_ms: Some(15_871),
                    failure_reason: Some("full_speed_to_stable_timeout"),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(tuned.approach_lead_ticks, 2);
        assert_eq!(
            tuned.approach_power_permille,
            previous.approach_power_permille
        );
        assert_eq!(
            tuned.approach_floor_power_permille,
            previous.approach_floor_power_permille
        );
        assert_eq!(
            tuned.brake_distance_centi_c,
            previous.brake_distance_centi_c
        );
        assert_eq!(tuned.hold_power_permille, previous.hold_power_permille);
    }

    #[test]
    fn thermal_stability_overshoot_fine_tunes_cutoff_without_advancing_low_temp_lead() {
        let mut previous = thermal_default_target_point(60);
        previous.brake_distance_centi_c = 1_110;
        previous.approach_damping_exponent_permille = 940;
        previous.approach_power_permille = 520;
        previous.approach_floor_power_permille = 400;
        previous.approach_lead_ticks = 6;
        previous.hold_power_permille = 170;
        previous.hold_reheat_power_permille = 440;
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 21_115,
                max_overshoot_c: 3.1,
                hold_peak_to_peak_c: 3.0,
                sample_count: 97,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_error_c: Some(-0.4),
                    hold_median_output_permille: Some(60),
                    hold_p90_output_permille: Some(270),
                    hold_mean_error_c: Some(-1.25),
                    hold_max_above_target_c: Some(3.1),
                    hold_max_below_target_c: Some(0.6),
                    hold_sample_count: 13,
                    residual_heat_after_hold_entry_c: Some(2.0),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis {
                    first_hold_at_ms: Some(18_000),
                    ..ThermalApproachGuardAnalysis::default()
                },
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    failure_reason: Some("full_speed_to_stable_timeout"),
                    warmup_exited_at_ms: Some(10_000),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(
            tuned.brake_distance_centi_c,
            previous.brake_distance_centi_c + 80
        );
        assert!(
            tuned.approach_damping_exponent_permille > previous.approach_damping_exponent_permille
        );
        assert_eq!(tuned.approach_lead_ticks, previous.approach_lead_ticks);
        assert!(tuned.overshoot_cutoff_centi_c < previous.overshoot_cutoff_centi_c);
        assert_eq!(tuned.approach_floor_power_permille, 400);
        assert_eq!(tuned.hold_power_permille, previous.hold_power_permille);
        assert_eq!(
            tuned.hold_reheat_power_permille,
            previous.hold_reheat_power_permille
        );
    }

    #[test]
    fn thermal_low_temp_bursty_hold_ripple_does_not_collapse_hold_power_to_zero() {
        let mut previous = thermal_default_target_point(60);
        previous.approach_power_permille = 450;
        previous.approach_floor_power_permille = 160;
        previous.approach_lead_ticks = 6;
        previous.brake_distance_centi_c = 1_100;
        previous.hold_entry_centi_c = 220;
        previous.hold_exit_centi_c = 400;
        previous.hold_power_permille = 40;
        previous.hold_reheat_power_permille = 200;
        previous.hold_kp_permille_per_c = 12;
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 16_991,
                max_overshoot_c: 0.8,
                hold_peak_to_peak_c: 3.2,
                sample_count: 770,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_error_c: Some(3.06),
                    hold_median_output_permille: Some(0),
                    hold_p90_output_permille: Some(130),
                    hold_mean_error_c: Some(0.69),
                    hold_max_above_target_c: Some(0.8),
                    hold_max_below_target_c: Some(3.42),
                    hold_sample_count: 640,
                    residual_heat_after_hold_entry_c: Some(3.86),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert_eq!(tuned.hold_power_permille, 70);
        assert_eq!(tuned.hold_reheat_power_permille, 200);
        assert_eq!(tuned.hold_kp_permille_per_c, 15);
    }

    #[test]
    fn thermal_low_temp_bursty_hold_ripple_stays_near_current_seed_on_mild_under_target() {
        let mut previous = thermal_default_target_point(60);
        previous.approach_power_permille = 450;
        previous.approach_floor_power_permille = 160;
        previous.approach_lead_ticks = 6;
        previous.brake_distance_centi_c = 1_100;
        previous.hold_entry_centi_c = 220;
        previous.hold_exit_centi_c = 400;
        previous.hold_power_permille = 40;
        previous.hold_reheat_power_permille = 200;
        previous.hold_kp_permille_per_c = 12;
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 20_106,
                max_overshoot_c: 1.05,
                hold_peak_to_peak_c: 3.61,
                sample_count: 802,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    approach_median_output_permille: Some(130),
                    first_hold_error_c: Some(1.68),
                    hold_median_output_permille: Some(0),
                    hold_p90_output_permille: Some(150),
                    hold_mean_error_c: Some(0.895),
                    hold_max_above_target_c: Some(1.05),
                    hold_max_below_target_c: Some(2.78),
                    hold_sample_count: 631,
                    residual_heat_after_hold_entry_c: Some(2.73),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert_eq!(tuned.hold_power_permille, 70);
        assert_eq!(tuned.hold_reheat_power_permille, 200);
        assert_eq!(tuned.hold_kp_permille_per_c, 15);
        assert_eq!(tuned.approach_floor_power_permille, 160);
    }

    #[test]
    fn thermal_late_low_temp_hold_entry_moves_hold_gate_earlier() {
        let mut previous = thermal_default_target_point(60);
        previous.approach_lead_ticks = 3;
        previous.overshoot_cutoff_centi_c = 110;
        previous.hold_entry_centi_c = 20;
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 25_693,
                max_overshoot_c: 1.9,
                hold_peak_to_peak_c: 1.5,
                sample_count: 260,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_error_c: Some(-0.4),
                    hold_max_above_target_c: Some(1.9),
                    hold_max_below_target_c: Some(0.0),
                    hold_sample_count: 17,
                    residual_heat_after_hold_entry_c: Some(1.5),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis {
                    first_hold_at_ms: Some(25_693),
                    ..ThermalApproachGuardAnalysis::default()
                },
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    failure_reason: Some("full_speed_to_stable_timeout"),
                    warmup_exited_at_ms: Some(16_047),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(tuned.approach_lead_ticks, previous.approach_lead_ticks);
        assert_eq!(
            tuned.overshoot_cutoff_centi_c,
            previous.overshoot_cutoff_centi_c
        );
        assert_eq!(tuned.hold_entry_centi_c, 60);
    }

    #[test]
    fn thermal_low_temp_hold_entry_carry_adds_hold_lead_without_rebraking() {
        let mut previous = thermal_default_target_point(60);
        previous.approach_lead_ticks = 3;
        previous.hold_lead_ticks = 0;
        previous.hold_power_permille = 60;
        previous.hold_reheat_power_permille = 90;
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 21_540,
                max_overshoot_c: 1.8,
                hold_peak_to_peak_c: 2.9,
                sample_count: 204,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_error_c: Some(0.6),
                    hold_max_above_target_c: Some(1.8),
                    hold_max_below_target_c: Some(1.1),
                    hold_p90_output_permille: Some(80),
                    hold_sample_count: 28,
                    residual_heat_after_hold_entry_c: Some(2.4),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis {
                    first_hold_at_ms: Some(21_201),
                    ..ThermalApproachGuardAnalysis::default()
                },
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    failure_reason: Some("full_speed_to_stable_timeout"),
                    warmup_exited_at_ms: Some(12_782),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(tuned.approach_lead_ticks, previous.approach_lead_ticks);
        assert_eq!(
            tuned.overshoot_cutoff_centi_c,
            previous.overshoot_cutoff_centi_c
        );
        assert_eq!(tuned.hold_lead_ticks, 2);
        assert_eq!(tuned.hold_reheat_power_permille, 60);
    }

    #[test]
    fn thermal_low_temp_bounded_entry_residual_prefers_overshoot_cutoff_trim() {
        let mut previous = thermal_default_target_point(100);
        previous.brake_distance_centi_c = 1_000;
        previous.approach_power_permille = 420;
        previous.approach_floor_power_permille = 300;
        previous.approach_damping_exponent_permille = 1_220;
        previous.approach_lead_ticks = 7;
        previous.hold_blend_ticks = 2;
        previous.hold_entry_centi_c = 150;
        previous.hold_exit_centi_c = 120;
        previous.hold_kp_permille_per_c = 20;
        previous.hold_lead_ticks = 8;
        previous.hold_power_permille = 220;
        previous.hold_reheat_power_permille = 220;
        previous.hold_off_centi_c = 180;
        previous.overshoot_cutoff_centi_c = 90;
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 100,
                rise_time_ms: 22_294,
                max_overshoot_c: 1.55,
                hold_peak_to_peak_c: 1.73,
                sample_count: 83,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_error_c: Some(0.88),
                    hold_max_above_target_c: Some(1.55),
                    hold_max_below_target_c: Some(0.88),
                    hold_p90_output_permille: Some(210),
                    hold_sample_count: 11,
                    residual_heat_after_hold_entry_c: Some(2.43),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis {
                    first_hold_at_ms: Some(21_652),
                    ..ThermalApproachGuardAnalysis::default()
                },
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    failure_reason: Some("full_speed_to_stable_timeout"),
                    warmup_exited_at_ms: Some(14_498),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(tuned.hold_lead_ticks, previous.hold_lead_ticks);
        assert_eq!(tuned.hold_power_permille, previous.hold_power_permille);
        assert_eq!(
            tuned.hold_reheat_power_permille,
            previous.hold_reheat_power_permille
        );
        assert_eq!(tuned.brake_distance_centi_c, 1080);
        assert_eq!(tuned.overshoot_cutoff_centi_c, 70);
        assert_eq!(tuned.hold_off_centi_c, 50);
        assert_eq!(tuned.hold_blend_ticks, 1);
        assert_eq!(tuned.hold_kp_permille_per_c, 16);
    }

    #[test]
    fn thermal_low_temp_moderate_residual_keeps_hold_exit_and_adds_brake() {
        let mut previous = thermal_default_target_point(60);
        previous.brake_distance_centi_c = 1_700;
        previous.approach_power_permille = 450;
        previous.approach_floor_power_permille = 220;
        previous.approach_damping_exponent_permille = 4_000;
        previous.approach_lead_ticks = 6;
        previous.hold_blend_ticks = 1;
        previous.hold_entry_centi_c = 180;
        previous.hold_exit_centi_c = 400;
        previous.hold_kp_permille_per_c = 8;
        previous.hold_power_permille = 135;
        previous.hold_reheat_power_permille = 140;
        previous.hold_off_centi_c = 50;
        previous.overshoot_cutoff_centi_c = 50;
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 16_197,
                max_overshoot_c: 1.68,
                hold_peak_to_peak_c: 2.15,
                sample_count: 215,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_error_c: Some(1.75),
                    hold_mean_error_c: Some(-0.041),
                    hold_max_above_target_c: Some(1.68),
                    hold_max_below_target_c: Some(1.86),
                    hold_median_output_permille: Some(0),
                    hold_p90_output_permille: Some(0),
                    hold_sample_count: 74,
                    residual_heat_after_hold_entry_c: Some(3.43),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis {
                    first_hold_at_ms: Some(14_203),
                    ..ThermalApproachGuardAnalysis::default()
                },
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    failure_reason: Some("full_speed_to_stable_timeout"),
                    warmup_exited_at_ms: Some(5_997),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(tuned.brake_distance_centi_c, 1_780);
        assert_eq!(tuned.hold_exit_centi_c, previous.hold_exit_centi_c);
        assert_eq!(tuned.approach_lead_ticks, previous.approach_lead_ticks);
        assert_eq!(
            tuned.approach_damping_exponent_permille,
            previous.approach_damping_exponent_permille
        );
        assert_eq!(
            tuned.overshoot_cutoff_centi_c,
            previous.overshoot_cutoff_centi_c
        );
    }

    #[test]
    fn thermal_low_temp_hold_entry_carry_trims_hold_power_once_hold_lead_is_maxed() {
        let mut previous = thermal_default_target_point(100);
        previous.brake_distance_centi_c = 1_000;
        previous.approach_power_permille = 420;
        previous.approach_floor_power_permille = 300;
        previous.approach_damping_exponent_permille = 1_220;
        previous.approach_lead_ticks = 7;
        previous.hold_blend_ticks = 2;
        previous.hold_entry_centi_c = 150;
        previous.hold_exit_centi_c = 120;
        previous.hold_kp_permille_per_c = 20;
        previous.hold_lead_ticks = 8;
        previous.hold_power_permille = 180;
        previous.hold_reheat_power_permille = 240;
        previous.hold_off_centi_c = 180;
        previous.overshoot_cutoff_centi_c = 90;
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 100,
                rise_time_ms: 22_294,
                max_overshoot_c: 2.1,
                hold_peak_to_peak_c: 2.45,
                sample_count: 83,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_error_c: Some(0.88),
                    hold_max_above_target_c: Some(2.1),
                    hold_max_below_target_c: Some(0.88),
                    hold_p90_output_permille: Some(260),
                    hold_sample_count: 11,
                    residual_heat_after_hold_entry_c: Some(2.43),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis {
                    first_hold_at_ms: Some(21_652),
                    ..ThermalApproachGuardAnalysis::default()
                },
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    failure_reason: Some("full_speed_to_stable_timeout"),
                    warmup_exited_at_ms: Some(14_498),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(tuned.hold_lead_ticks, 8);
        assert_eq!(tuned.hold_power_permille, 160);
        assert_eq!(tuned.hold_reheat_power_permille, 210);
        assert_eq!(tuned.hold_off_centi_c, 200);
        assert_eq!(tuned.approach_lead_ticks, previous.approach_lead_ticks);
    }

    #[test]
    fn thermal_severe_residual_heat_brakes_earlier_and_lowers_approach_floor() {
        let mut previous = thermal_default_target_point(60);
        previous.brake_distance_centi_c = 1_210;
        previous.approach_damping_exponent_permille = 1_040;
        previous.approach_power_permille = 520;
        previous.approach_floor_power_permille = 400;
        previous.approach_lead_ticks = 7;
        previous.hold_power_permille = 90;
        previous.hold_reheat_power_permille = 340;
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 18_511,
                max_overshoot_c: 7.7,
                hold_peak_to_peak_c: 6.8,
                sample_count: 92,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_error_c: Some(-0.9),
                    hold_median_output_permille: Some(0),
                    hold_mean_error_c: Some(-4.61),
                    hold_max_above_target_c: Some(7.7),
                    hold_max_below_target_c: Some(0.0),
                    hold_sample_count: 19,
                    residual_heat_after_hold_entry_c: Some(6.8),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    failure_reason: Some("full_speed_to_stable_timeout"),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(tuned.brake_distance_centi_c, 1_560);
        assert_eq!(tuned.approach_damping_exponent_permille, 1_240);
        assert_eq!(tuned.approach_lead_ticks, 9);
        assert_eq!(tuned.approach_floor_power_permille, 400);
        assert_eq!(tuned.hold_reheat_power_permille, 340);
        assert_eq!(tuned.hold_power_permille, 90);
    }

    #[test]
    fn thermal_fast_low_temp_residual_reduces_warmup_power() {
        let mut previous = thermal_default_target_point(60);
        previous.warmup_power_permille = 1_000;
        previous.approach_power_permille = 500;
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 12_000,
                max_overshoot_c: 9.0,
                hold_peak_to_peak_c: 9.0,
                sample_count: 120,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    approach_median_slope_c_per_s: Some(3.0),
                    hold_sample_count: 120,
                    hold_max_above_target_c: Some(9.0),
                    residual_heat_after_hold_entry_c: Some(6.0),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert_eq!(tuned.warmup_power_permille, 750);
    }

    #[test]
    fn thermal_bounded_residual_heat_advances_coast_gate_without_moving_warmup_exit() {
        let mut previous = thermal_default_target_point(60);
        previous.brake_distance_centi_c = 1_910;
        previous.hold_exit_centi_c = 200;
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 19_708,
                max_overshoot_c: 3.3,
                hold_peak_to_peak_c: 2.4,
                sample_count: 85,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    hold_max_above_target_c: Some(3.3),
                    hold_max_below_target_c: Some(0.0),
                    hold_sample_count: 7,
                    residual_heat_after_hold_entry_c: Some(2.4),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    failure_reason: Some("full_speed_to_stable_timeout"),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(tuned.brake_distance_centi_c, 1_910);
        assert_eq!(tuned.hold_exit_centi_c, 380);
    }

    #[test]
    fn thermal_high_temp_entry_residual_advances_braking_instead_of_raising_hold_power() {
        let mut previous = thermal_default_target_point(220);
        previous.brake_distance_centi_c = 520;
        previous.approach_power_permille = 760;
        previous.approach_floor_power_permille = 720;
        previous.approach_damping_exponent_permille = 550;
        previous.approach_lead_ticks = 2;
        previous.hold_power_permille = 700;
        previous.hold_reheat_power_permille = 780;
        previous.hold_kp_permille_per_c = 19;

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 220,
                rise_time_ms: 93_722,
                max_overshoot_c: 3.0,
                hold_peak_to_peak_c: 5.4,
                sample_count: 1_476,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_temp_c: Some(220.6),
                    first_hold_error_c: Some(-0.6),
                    residual_heat_after_hold_entry_c: Some(2.4),
                    approach_median_output_permille: Some(780),
                    approach_median_slope_c_per_s: Some(6.14),
                    hold_median_output_permille: Some(750),
                    hold_p90_output_permille: Some(790),
                    hold_mean_error_c: Some(0.44),
                    hold_max_above_target_c: Some(3.0),
                    hold_max_below_target_c: Some(2.4),
                    approach_sample_count: 65,
                    hold_sample_count: 541,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert!(tuned.brake_distance_centi_c > previous.brake_distance_centi_c);
        assert!(tuned.approach_lead_ticks > previous.approach_lead_ticks);
        assert!(
            tuned.approach_damping_exponent_permille > previous.approach_damping_exponent_permille
        );
        assert!(tuned.hold_power_permille <= 720);
        assert!(tuned.hold_reheat_power_permille <= previous.hold_reheat_power_permille);
    }

    #[test]
    fn thermal_replay_accepts_missing_hold_metric_for_incomplete_stage() {
        let stage = thermal_stage_result_from_value(&json!({
            "targetTempC": 60,
            "riseTimeMs": 17_680,
            "maxOvershootC": 0.0,
            "holdPeakToPeakC": null,
            "sampleCount": 71,
            "stopReason": "full_speed_to_stable_timeout",
        }))
        .unwrap();

        assert!(stage.hold_peak_to_peak_c.is_infinite());
    }

    #[test]
    fn thermal_replay_accepts_warmup_timeout_stop_reason() {
        let stage = thermal_stage_result_from_value(&json!({
            "targetTempC": 220,
            "riseTimeMs": 45_000,
            "maxOvershootC": 0.0,
            "holdPeakToPeakC": null,
            "sampleCount": 90,
            "stopReason": "warmup_timeout",
        }))
        .unwrap();

        assert_eq!(stage.stop_reason, "warmup_timeout");
        assert!(stage.hold_peak_to_peak_c.is_infinite());
    }

    #[test]
    fn thermal_sample_rate_accepts_measured_four_hz_sequence_with_one_jitter_gap() {
        let mut tracker = ThermalSampleRateTracker::new();
        let mut observation = tracker.observe(73);
        for elapsed_ms in [
            322, 557, 800, 1_067, 1_305, 1_571, 1_816, 2_074, 2_321, 2_575, 2_824, 3_068, 3_322,
            3_561, 3_925,
        ] {
            observation = tracker.observe(elapsed_ms);
            assert!(!observation.violation);
        }
        assert!(observation.rolling_rate_hz.unwrap() >= THERMAL_MIN_SAMPLE_RATE_HZ);
    }

    #[test]
    fn thermal_sample_rate_rejects_sustained_sub_three_hz_sampling() {
        let mut tracker = ThermalSampleRateTracker::new();
        let mut observation = tracker.observe(0);
        for elapsed_ms in [
            350, 700, 1_050, 1_400, 1_750, 2_100, 2_450, 2_800, 3_150, 3_500, 3_850, 4_200, 4_550,
            4_900, 5_250, 5_600, 5_950, 6_300,
        ] {
            observation = tracker.observe(elapsed_ms);
        }
        assert!(observation.violation);
        assert!(observation.rolling_rate_hz.unwrap() < THERMAL_MIN_SAMPLE_RATE_HZ);
    }

    #[test]
    fn thermal_sample_rate_tolerates_recorded_single_serial_stall() {
        let mut tracker = ThermalSampleRateTracker::new();
        let mut observation = tracker.observe(226);
        for elapsed_ms in [
            315, 412, 503, 596, 704, 816, 937, 1_070, 1_200, 1_313, 1_394, 1_525, 1_593, 1_667,
            1_735, 1_845, 1_946, 2_063, 2_325, 2_446, 2_643, 2_810, 2_912, 3_279, 3_528, 3_694,
            3_925, 4_133, 4_486, 4_738, 5_102, 6_258,
        ] {
            observation = tracker.observe(elapsed_ms);
        }
        assert!(!observation.violation);
        assert!(observation.rolling_rate_hz.unwrap() < THERMAL_MIN_SAMPLE_RATE_HZ);
    }

    #[test]
    fn thermal_sample_rate_tolerates_one_transient_low_rate_window() {
        let mut tracker = ThermalSampleRateTracker::new();
        let mut observation = tracker.observe(0);
        for elapsed_ms in [
            250, 500, 750, 1_000, 1_250, 1_500, 2_300, 2_400, 2_500, 2_600, 2_700, 2_800, 2_900,
            3_000,
        ] {
            observation = tracker.observe(elapsed_ms);
        }
        assert!(observation.rolling_rate_hz.unwrap() >= THERMAL_MIN_SAMPLE_RATE_HZ);
        assert!(!observation.violation);
    }

    #[test]
    fn thermal_sample_rate_tolerates_one_isolated_sampling_stall() {
        let mut tracker = ThermalSampleRateTracker::new();
        let mut observation = tracker.observe(0);
        for elapsed_ms in [
            250, 1_251, 1_350, 1_450, 1_550, 1_650, 1_750, 1_850, 1_950, 2_050, 2_150, 2_250,
            2_350, 2_450, 2_550, 2_650, 2_750, 2_850, 2_950, 3_050,
        ] {
            observation = tracker.observe(elapsed_ms);
        }
        assert!(!observation.violation);
        assert!(observation.rolling_rate_hz.unwrap() >= THERMAL_MIN_SAMPLE_RATE_HZ);
    }

    #[test]
    fn thermal_timeout_tuning_raises_power_and_reduces_brake() {
        let previous = thermal_default_target_point(250);
        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 250,
                rise_time_ms: 180_000,
                max_overshoot_c: 3.5,
                hold_peak_to_peak_c: f64::INFINITY,
                sample_count: 120,
                stop_reason: "timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis::default(),
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert!(tuned.approach_power_permille >= previous.approach_power_permille);
        assert!(tuned.approach_floor_power_permille >= previous.approach_floor_power_permille);
        assert!(tuned.hold_power_permille >= previous.hold_power_permille);
        assert!(tuned.brake_distance_centi_c <= previous.brake_distance_centi_c);
    }

    #[test]
    fn thermal_high_temp_power_limit_converges_to_saturated_near_target_profile() {
        let mut previous = thermal_default_target_point(220);
        previous.brake_distance_centi_c = 442;
        previous.warmup_power_permille = 980;
        previous.approach_power_permille = 940;
        previous.approach_floor_power_permille = 760;
        previous.approach_damping_exponent_permille = 250;
        previous.approach_lead_ticks = 2;
        previous.hold_power_permille = 750;
        previous.hold_reheat_power_permille = 850;
        previous.hold_entry_centi_c = 28;
        previous.hold_exit_centi_c = 70;
        previous.hold_off_centi_c = 170;
        previous.overshoot_cutoff_centi_c = 275;

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 220,
                rise_time_ms: 154_721,
                max_overshoot_c: 0.0,
                hold_peak_to_peak_c: f64::INFINITY,
                sample_count: 465,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    approach_median_output_permille: Some(930),
                    approach_median_slope_c_per_s: Some(0.75),
                    approach_sample_count: 55,
                    hold_sample_count: 0,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis {
                    approach_started_at_ms: Some(144_299),
                    ..ThermalApproachGuardAnalysis::default()
                },
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    warmup_exited_at_ms: Some(144_299),
                    failure_reason: Some("full_speed_to_stable_timeout"),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(tuned.brake_distance_centi_c, 160);
        assert_eq!(tuned.hold_entry_centi_c, 150);
        assert_eq!(tuned.hold_exit_centi_c, 160);
        assert_eq!(tuned.warmup_power_permille, 1_000);
        assert_eq!(tuned.approach_power_permille, 1_000);
        assert_eq!(tuned.approach_floor_power_permille, 1_000);
        assert_eq!(tuned.approach_lead_ticks, 0);
        assert_eq!(tuned.hold_power_permille, 980);
        assert_eq!(tuned.hold_reheat_power_permille, 1_000);
        assert_eq!(tuned.hold_off_centi_c, 80);
        assert_eq!(tuned.overshoot_cutoff_centi_c, 180);
    }

    #[test]
    fn thermal_pre_hold_timeout_reduces_excessive_predictive_lead() {
        let mut previous = thermal_default_target_point(60);
        previous.approach_lead_ticks = 14;
        previous.brake_distance_centi_c = 1_640;
        previous.approach_floor_power_permille = 90;

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 60,
                rise_time_ms: 15_683,
                max_overshoot_c: 0.0,
                hold_peak_to_peak_c: f64::INFINITY,
                sample_count: 63,
                stop_reason: "full_speed_to_stable_timeout",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    approach_median_output_permille: Some(230),
                    approach_median_slope_c_per_s: Some(3.58),
                    approach_sample_count: 13,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis {
                    approach_started_at_ms: Some(5_385),
                    ..ThermalApproachGuardAnalysis::default()
                },
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    warmup_exited_at_ms: Some(5_385),
                    failure_reason: Some("full_speed_to_stable_timeout"),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(tuned.approach_lead_ticks, 7);
        assert_eq!(tuned.brake_distance_centi_c, 1_520);
        assert!(tuned.approach_floor_power_permille > previous.approach_floor_power_permille);
    }

    #[test]
    fn thermal_overshoot_tuning_keeps_or_increases_lead_and_brake() {
        let mut previous = thermal_default_target_point(100);
        previous.approach_lead_ticks = 5;

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 100,
                rise_time_ms: 31_000,
                max_overshoot_c: 7.9,
                hold_peak_to_peak_c: 7.5,
                sample_count: 300,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_temp_c: Some(100.4),
                    first_hold_error_c: Some(-0.4),
                    residual_heat_after_hold_entry_c: Some(7.5),
                    approach_median_output_permille: Some(0),
                    approach_median_slope_c_per_s: Some(3.0),
                    hold_median_output_permille: Some(0),
                    hold_p90_output_permille: Some(110),
                    hold_mean_error_c: Some(-3.6),
                    hold_max_above_target_c: Some(7.9),
                    hold_max_below_target_c: Some(0.0),
                    approach_sample_count: 20,
                    hold_sample_count: 240,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert!(tuned.brake_distance_centi_c > previous.brake_distance_centi_c);
        assert!(tuned.approach_lead_ticks >= previous.approach_lead_ticks);
        assert!(tuned.approach_floor_power_permille <= previous.approach_floor_power_permille);
    }

    #[test]
    fn thermal_low_temp_overshoot_prefers_more_lead_over_collapsing_power() {
        let previous = ThermalCandidatePoint {
            target_temp_c: 100,
            brake_distance_centi_c: 1_180,
            warmup_power_permille: 260,
            approach_power_permille: 181,
            approach_floor_power_permille: 99,
            approach_damping_exponent_permille: 1_000,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 58,
            hold_reheat_power_permille: 116,
            hold_entry_centi_c: 35,
            hold_exit_centi_c: 93,
            hold_on_centi_c: 30,
            hold_off_centi_c: 105,
            overshoot_cutoff_centi_c: 151,
            hold_kp_permille_per_c: 13,
            hold_ki_permille_per_c_tick: 2,
            hold_blend_ticks: 9,
            approach_lead_ticks: 3,
            hold_lead_ticks: 0,
        };

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 100,
                rise_time_ms: 35_531,
                max_overshoot_c: 10.0,
                hold_peak_to_peak_c: 10.2,
                sample_count: 383,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_temp_c: Some(101.4),
                    first_hold_error_c: Some(-1.4),
                    residual_heat_after_hold_entry_c: Some(8.6),
                    approach_median_output_permille: Some(140),
                    approach_median_slope_c_per_s: Some(9.66),
                    hold_median_output_permille: Some(0),
                    hold_p90_output_permille: Some(10),
                    hold_mean_error_c: Some(-5.8),
                    hold_max_above_target_c: Some(10.0),
                    hold_max_below_target_c: Some(0.2),
                    approach_sample_count: 19,
                    hold_sample_count: 242,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert!(tuned.brake_distance_centi_c > previous.brake_distance_centi_c);
        assert!(tuned.approach_lead_ticks > previous.approach_lead_ticks);
        assert!(tuned.approach_floor_power_permille >= 99);
        assert!(tuned.approach_power_permille >= tuned.approach_floor_power_permille);
        assert!(tuned.hold_power_permille <= previous.hold_power_permille);
        assert!(tuned.hold_reheat_power_permille >= tuned.approach_floor_power_permille);
    }

    #[test]
    fn thermal_mid_temp_overshoot_increases_braking_and_softens_reheat() {
        let previous = ThermalCandidatePoint {
            target_temp_c: 140,
            brake_distance_centi_c: 2_180,
            warmup_power_permille: 240,
            approach_power_permille: 160,
            approach_floor_power_permille: 40,
            approach_damping_exponent_permille: 1_000,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 0,
            hold_reheat_power_permille: 80,
            hold_entry_centi_c: 30,
            hold_exit_centi_c: 87,
            hold_on_centi_c: 30,
            hold_off_centi_c: 99,
            overshoot_cutoff_centi_c: 148,
            hold_kp_permille_per_c: 8,
            hold_ki_permille_per_c_tick: 2,
            hold_blend_ticks: 8,
            approach_lead_ticks: 18,
            hold_lead_ticks: 0,
        };

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 140,
                rise_time_ms: 83_266,
                max_overshoot_c: 3.2,
                hold_peak_to_peak_c: 4.1,
                sample_count: 625,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_temp_c: Some(140.3),
                    first_hold_error_c: Some(-0.3),
                    residual_heat_after_hold_entry_c: Some(2.9),
                    approach_median_output_permille: Some(40),
                    approach_median_slope_c_per_s: Some(2.49),
                    hold_median_output_permille: Some(0),
                    hold_p90_output_permille: Some(70),
                    hold_mean_error_c: Some(-0.88),
                    hold_max_above_target_c: Some(3.2),
                    hold_max_below_target_c: Some(0.9),
                    approach_sample_count: 138,
                    hold_sample_count: 248,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert!(tuned.brake_distance_centi_c > previous.brake_distance_centi_c);
        assert!(tuned.approach_power_permille <= previous.approach_power_permille);
        assert!(tuned.approach_floor_power_permille <= previous.approach_floor_power_permille);
        assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille);
    }

    #[test]
    fn thermal_mid_temp_hold_swing_does_not_relax_braking() {
        let previous = ThermalCandidatePoint {
            target_temp_c: 140,
            brake_distance_centi_c: 2_471,
            warmup_power_permille: 360,
            approach_power_permille: 140,
            approach_floor_power_permille: 20,
            approach_damping_exponent_permille: 1_000,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 0,
            hold_reheat_power_permille: 58,
            hold_entry_centi_c: 35,
            hold_exit_centi_c: 107,
            hold_on_centi_c: 30,
            hold_off_centi_c: 99,
            overshoot_cutoff_centi_c: 150,
            hold_kp_permille_per_c: 8,
            hold_ki_permille_per_c_tick: 1,
            hold_blend_ticks: 12,
            approach_lead_ticks: 18,
            hold_lead_ticks: 0,
        };

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 140,
                rise_time_ms: 92_108,
                max_overshoot_c: 2.6,
                hold_peak_to_peak_c: 3.5,
                sample_count: 642,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_temp_c: Some(140.3),
                    first_hold_error_c: Some(-0.3),
                    residual_heat_after_hold_entry_c: Some(2.3),
                    approach_median_output_permille: Some(0),
                    approach_median_slope_c_per_s: Some(3.38),
                    hold_median_output_permille: Some(0),
                    hold_p90_output_permille: Some(60),
                    hold_mean_error_c: Some(-0.58),
                    hold_max_above_target_c: Some(2.6),
                    hold_max_below_target_c: Some(0.9),
                    approach_sample_count: 144,
                    hold_sample_count: 247,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert!(tuned.brake_distance_centi_c >= previous.brake_distance_centi_c);
        assert!(tuned.approach_lead_ticks <= previous.approach_lead_ticks);
        assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille);
    }

    #[test]
    fn thermal_tuning_clamps_extreme_power_targets_without_panicking() {
        let previous = ThermalCandidatePoint {
            target_temp_c: 100,
            brake_distance_centi_c: 1_500,
            warmup_power_permille: 900,
            approach_power_permille: 900,
            approach_floor_power_permille: 900,
            approach_damping_exponent_permille: 1_000,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 980,
            hold_reheat_power_permille: 980,
            hold_entry_centi_c: 25,
            hold_exit_centi_c: 75,
            hold_on_centi_c: 30,
            hold_off_centi_c: 200,
            overshoot_cutoff_centi_c: 350,
            hold_kp_permille_per_c: 18,
            hold_ki_permille_per_c_tick: 2,
            hold_blend_ticks: 10,
            approach_lead_ticks: 4,
            hold_lead_ticks: 0,
        };

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 100,
                rise_time_ms: 40_000,
                max_overshoot_c: 0.0,
                hold_peak_to_peak_c: 3.1,
                sample_count: 200,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_temp_c: Some(99.9),
                    first_hold_error_c: Some(0.1),
                    residual_heat_after_hold_entry_c: Some(0.1),
                    approach_median_output_permille: Some(1_000),
                    approach_median_slope_c_per_s: Some(0.2),
                    hold_median_output_permille: Some(1_000),
                    hold_p90_output_permille: Some(1_000),
                    hold_mean_error_c: Some(2.5),
                    hold_max_above_target_c: Some(0.0),
                    hold_max_below_target_c: Some(4.0),
                    approach_sample_count: 40,
                    hold_sample_count: 240,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert_eq!(tuned.hold_power_permille, 1_000);
        assert!(tuned.approach_floor_power_permille <= 1_000);
        assert!(tuned.approach_power_permille <= 1_000);
        assert!(tuned.warmup_power_permille <= 1_000);
    }

    #[test]
    fn thermal_under_target_hold_swing_does_not_collapse_sustain_power() {
        let previous = ThermalCandidatePoint {
            target_temp_c: 100,
            brake_distance_centi_c: 1_286,
            warmup_power_permille: 390,
            approach_power_permille: 260,
            approach_floor_power_permille: 275,
            approach_damping_exponent_permille: 975,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 59,
            hold_reheat_power_permille: 390,
            hold_entry_centi_c: 25,
            hold_exit_centi_c: 75,
            hold_on_centi_c: 30,
            hold_off_centi_c: 198,
            overshoot_cutoff_centi_c: 353,
            hold_kp_permille_per_c: 18,
            hold_ki_permille_per_c_tick: 2,
            hold_blend_ticks: 10,
            approach_lead_ticks: 4,
            hold_lead_ticks: 0,
        };

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 100,
                rise_time_ms: 101_928,
                max_overshoot_c: 2.0,
                hold_peak_to_peak_c: 4.3,
                sample_count: 625,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_temp_c: Some(99.8),
                    first_hold_error_c: Some(0.2),
                    residual_heat_after_hold_entry_c: Some(2.2),
                    approach_median_output_permille: Some(270),
                    approach_median_slope_c_per_s: Some(1.976),
                    hold_median_output_permille: Some(70),
                    hold_p90_output_permille: Some(390),
                    hold_mean_error_c: Some(0.056),
                    hold_max_above_target_c: Some(2.0),
                    hold_max_below_target_c: Some(2.3),
                    approach_sample_count: 161,
                    hold_sample_count: 218,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert!(tuned.brake_distance_centi_c >= previous.brake_distance_centi_c);
        assert!(tuned.approach_floor_power_permille >= previous.approach_floor_power_permille);
        assert!(tuned.approach_power_permille >= previous.approach_power_permille);
        assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille.saturating_add(100));
        assert!(tuned.approach_lead_ticks >= previous.approach_lead_ticks);
        assert!(
            tuned.approach_damping_exponent_permille >= previous.approach_damping_exponent_permille
        );
    }

    #[test]
    fn thermal_high_temp_hold_ripple_rebases_equilibrium_without_weakening_approach() {
        let previous = ThermalCandidatePoint {
            target_temp_c: 220,
            brake_distance_centi_c: 442,
            warmup_power_permille: 980,
            approach_power_permille: 940,
            approach_floor_power_permille: 760,
            approach_damping_exponent_permille: 250,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 720,
            hold_reheat_power_permille: 880,
            hold_entry_centi_c: 8,
            hold_exit_centi_c: 45,
            hold_on_centi_c: 14,
            hold_off_centi_c: 205,
            overshoot_cutoff_centi_c: 320,
            hold_kp_permille_per_c: 34,
            hold_ki_permille_per_c_tick: 1,
            hold_blend_ticks: 4,
            approach_lead_ticks: 2,
            hold_lead_ticks: 0,
        };

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 220,
                rise_time_ms: 61_499,
                max_overshoot_c: 3.0,
                hold_peak_to_peak_c: 4.8,
                sample_count: 487,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    approach_median_output_permille: Some(890),
                    approach_median_slope_c_per_s: Some(2.1978),
                    approach_sample_count: 147,
                    first_hold_temp_c: Some(221.0),
                    first_hold_error_c: Some(-1.0),
                    hold_max_above_target_c: Some(3.0),
                    hold_max_below_target_c: Some(1.8),
                    hold_mean_error_c: Some(0.2719),
                    hold_median_output_permille: Some(760),
                    hold_p90_output_permille: Some(890),
                    hold_sample_count: 242,
                    residual_heat_after_hold_entry_c: Some(2.0),
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert_eq!(
            tuned.approach_power_permille,
            previous.approach_power_permille
        );
        assert!(tuned.approach_floor_power_permille >= tuned.hold_power_permille);
        assert!(tuned.brake_distance_centi_c > previous.brake_distance_centi_c);
        assert!(tuned.approach_lead_ticks > previous.approach_lead_ticks);
        assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille);
        assert!(tuned.hold_blend_ticks < previous.hold_blend_ticks);
    }

    #[test]
    fn thermal_saturated_high_temp_ripple_widens_taper_instead_of_cutting_power_deeply() {
        let mut previous = thermal_default_target_point(220);
        previous.approach_power_permille = 1_000;
        previous.approach_floor_power_permille = 1_000;
        previous.hold_power_permille = 990;
        previous.hold_reheat_power_permille = 1_000;
        previous.hold_on_centi_c = 75;
        previous.hold_off_centi_c = 50;
        previous.overshoot_cutoff_centi_c = 180;
        previous.hold_blend_ticks = 4;

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 220,
                rise_time_ms: 123_956,
                max_overshoot_c: 1.5,
                hold_peak_to_peak_c: 3.2,
                sample_count: 614,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_temp_c: Some(219.7),
                    first_hold_error_c: Some(0.3),
                    residual_heat_after_hold_entry_c: Some(1.8),
                    approach_median_output_permille: Some(1_000),
                    approach_median_slope_c_per_s: Some(1.0),
                    hold_median_output_permille: Some(990),
                    hold_p90_output_permille: Some(1_000),
                    hold_mean_error_c: Some(0.11),
                    hold_max_above_target_c: Some(1.5),
                    hold_max_below_target_c: Some(1.7),
                    approach_sample_count: 30,
                    hold_sample_count: 201,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert_eq!(tuned.hold_power_permille, 990);
        assert_eq!(tuned.hold_off_centi_c, 50);
        assert_eq!(tuned.hold_on_centi_c, 75);
        assert_eq!(tuned.overshoot_cutoff_centi_c, 383);
        assert_eq!(tuned.hold_blend_ticks, 4);
    }

    #[test]
    fn thermal_high_temp_entry_carry_ripple_lowers_hold_on_and_blend() {
        let previous = ThermalCandidatePoint {
            target_temp_c: 220,
            brake_distance_centi_c: 500,
            warmup_power_permille: 980,
            approach_power_permille: 920,
            approach_floor_power_permille: 730,
            approach_damping_exponent_permille: 250,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 740,
            hold_reheat_power_permille: 790,
            hold_entry_centi_c: 100,
            hold_exit_centi_c: 170,
            hold_on_centi_c: 200,
            hold_off_centi_c: 250,
            overshoot_cutoff_centi_c: 275,
            hold_kp_permille_per_c: 26,
            hold_ki_permille_per_c_tick: 2,
            hold_blend_ticks: 10,
            approach_lead_ticks: 0,
            hold_lead_ticks: 0,
        };

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 220,
                rise_time_ms: 143_139,
                max_overshoot_c: 1.7,
                hold_peak_to_peak_c: 4.1,
                sample_count: 1_154,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_temp_c: Some(219.6),
                    first_hold_error_c: Some(0.4),
                    residual_heat_after_hold_entry_c: Some(2.1),
                    approach_median_output_permille: Some(810),
                    approach_median_slope_c_per_s: Some(2.43),
                    hold_median_output_permille: Some(810),
                    hold_p90_output_permille: Some(850),
                    hold_mean_error_c: Some(0.71),
                    hold_max_above_target_c: Some(1.7),
                    hold_max_below_target_c: Some(2.4),
                    approach_sample_count: 465,
                    hold_sample_count: 243,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert_eq!(
            tuned.approach_power_permille,
            previous.approach_power_permille
        );
        assert!(tuned.approach_floor_power_permille <= tuned.hold_reheat_power_permille);
        assert_eq!(tuned.hold_entry_centi_c, previous.hold_entry_centi_c);
        assert_eq!(tuned.hold_exit_centi_c, previous.hold_exit_centi_c);
        assert!(tuned.hold_on_centi_c < previous.hold_on_centi_c);
        assert!(tuned.hold_on_centi_c <= 160);
        assert!(tuned.hold_off_centi_c < previous.hold_off_centi_c);
        assert!(tuned.hold_blend_ticks < previous.hold_blend_ticks);
        assert!(tuned.hold_blend_ticks <= 6);
        assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille);
        assert!(tuned.hold_kp_permille_per_c > previous.hold_kp_permille_per_c);
    }

    #[test]
    fn thermal_passing_stage_keeps_existing_candidate() {
        let previous = ThermalCandidatePoint {
            target_temp_c: 100,
            brake_distance_centi_c: 1286,
            warmup_power_permille: 390,
            approach_power_permille: 260,
            approach_floor_power_permille: 275,
            approach_damping_exponent_permille: 975,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 59,
            hold_reheat_power_permille: 390,
            hold_entry_centi_c: 25,
            hold_exit_centi_c: 75,
            hold_on_centi_c: 30,
            hold_off_centi_c: 198,
            overshoot_cutoff_centi_c: 353,
            hold_kp_permille_per_c: 18,
            hold_ki_permille_per_c_tick: 2,
            hold_blend_ticks: 10,
            approach_lead_ticks: 4,
            hold_lead_ticks: 0,
        };
        let result = ThermalStageResult {
            target_temp_c: 100,
            rise_time_ms: 114_668,
            max_overshoot_c: 0.0,
            hold_peak_to_peak_c: 2.1,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            sample_count: 686,
            analysis: ThermalStageAnalysis {
                approach_sample_count: 343,
                approach_median_output_permille: Some(220),
                approach_median_slope_c_per_s: Some(1.82),
                first_hold_temp_c: Some(99.8),
                first_hold_error_c: Some(0.2),
                hold_sample_count: 241,
                hold_mean_error_c: Some(1.68),
                hold_max_below_target_c: Some(2.3),
                hold_max_above_target_c: Some(0.0),
                hold_median_output_permille: Some(220),
                hold_p90_output_permille: Some(230),
                residual_heat_after_hold_entry_c: Some(0.0),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        };

        let tuned = tune_thermal_candidate_point(previous, &result);

        assert_eq!(tuned, previous);
    }

    #[test]
    fn thermal_high_temp_deep_hold_drop_widens_residency_and_adds_lead() {
        let mut previous = thermal_default_target_point(220);
        previous.hold_exit_centi_c = 50;
        previous.hold_lead_ticks = 0;

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 220,
                rise_time_ms: 69_621,
                max_overshoot_c: 1.7,
                hold_peak_to_peak_c: 6.2,
                sample_count: 1_136,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_temp_c: Some(221.3),
                    first_hold_error_c: Some(-1.3),
                    residual_heat_after_hold_entry_c: Some(0.4),
                    approach_median_output_permille: Some(740),
                    approach_median_slope_c_per_s: Some(3.04),
                    hold_median_output_permille: Some(730),
                    hold_p90_output_permille: Some(750),
                    hold_mean_error_c: Some(1.78),
                    hold_max_above_target_c: Some(1.7),
                    hold_max_below_target_c: Some(4.5),
                    approach_sample_count: 85,
                    hold_sample_count: 467,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                    settle_time_ms: Some(8_425),
                    ..ThermalFullSpeedStableAnalysis::default()
                },
            },
        );

        assert_eq!(tuned.hold_exit_centi_c, 300);
        assert_eq!(tuned.hold_lead_ticks, 1);
        assert!(tuned.hold_power_permille >= 700);
        assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille);
    }

    #[test]
    fn thermal_replay_analysis_recovers_positive_approach_slope() {
        let samples = vec![
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "warmup",
                "elapsedMs": 0,
                "heaterTelemetry": { "currentTempC": 92.0, "heaterOutputPercent": 40 },
                "status": { "heaterControlPhase": "approach" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "warmup",
                "elapsedMs": 1_000,
                "heaterTelemetry": { "currentTempC": 94.0, "heaterOutputPercent": 30 },
                "status": { "heaterControlPhase": "approach" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "warmup",
                "elapsedMs": 2_000,
                "heaterTelemetry": { "currentTempC": 96.2, "heaterOutputPercent": 20 },
                "status": { "heaterControlPhase": "approach" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "hold",
                "elapsedMs": 3_000,
                "heaterTelemetry": { "currentTempC": 100.4, "heaterOutputPercent": 15 },
                "status": { "heaterControlPhase": "hold" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "hold",
                "elapsedMs": 4_000,
                "heaterTelemetry": { "currentTempC": 103.8, "heaterOutputPercent": 0 },
                "status": { "heaterControlPhase": "hold" },
            }),
        ];

        let stage_samples = thermal_replay_stage_samples(&samples, 100).unwrap();
        let analysis = thermal_replay_stage_analysis(&stage_samples, 100);

        assert!(analysis.approach_median_slope_c_per_s.unwrap_or(0.0) > 0.5);
        assert_eq!(analysis.first_hold_temp_c, Some(100.4));
        assert!(analysis.residual_heat_after_hold_entry_c.unwrap_or(0.0) > 3.0);
        assert_eq!(
            analysis.approach_curve_fit_basis,
            Some("target_error_from_approach_start")
        );
        assert_eq!(
            analysis.approach_curve_preferred_ms,
            Some(THERMAL_APPROACH_CURVE_PREFERRED_MS)
        );
        assert_eq!(
            analysis.approach_curve_limit_ms,
            Some(THERMAL_APPROACH_CURVE_LIMIT_MS)
        );
    }

    #[test]
    fn thermal_replay_analysis_classifies_underpowered_approach_curve() {
        let samples = vec![
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "warmup",
                "elapsedMs": 0,
                "heaterTelemetry": { "currentTempC": 92.0, "heaterOutputPercent": 40 },
                "status": { "heaterControlPhase": "approach" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "warmup",
                "elapsedMs": 2_000,
                "heaterTelemetry": { "currentTempC": 93.5, "heaterOutputPercent": 30 },
                "status": { "heaterControlPhase": "approach" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "warmup",
                "elapsedMs": 6_000,
                "heaterTelemetry": { "currentTempC": 95.0, "heaterOutputPercent": 20 },
                "status": { "heaterControlPhase": "approach" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "warmup",
                "elapsedMs": 10_000,
                "heaterTelemetry": { "currentTempC": 96.1, "heaterOutputPercent": 10 },
                "status": { "heaterControlPhase": "approach" },
            }),
        ];

        let stage_samples = thermal_replay_stage_samples(&samples, 100).unwrap();
        let analysis = thermal_replay_stage_analysis(&stage_samples, 100);

        assert_eq!(
            analysis.approach_curve_deviation_class,
            Some("underpowered_or_early_coast")
        );
        assert_eq!(
            analysis.approach_curve_fitted_ms,
            Some(THERMAL_APPROACH_CURVE_LIMIT_MS)
        );
        assert!(analysis.approach_curve_max_below_c.unwrap_or_default() > 3.0);
        assert_eq!(analysis.approach_curve_tail_uses_half_floor, Some(true));
    }

    #[test]
    fn thermal_replay_source_analysis_summarizes_approach_and_hold_windows() {
        let samples = vec![
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "warmup",
                "elapsedMs": 0,
                "sourceTelemetry": { "voltageMv": 21_000, "currentMa": 3_000, "powerMw": 63_000 },
                "heaterTelemetry": { "currentTempC": 92.0, "heaterOutputPercent": 40 },
                "status": { "heaterControlPhase": "approach" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "warmup",
                "elapsedMs": 1_000,
                "sourceTelemetry": { "voltageMv": 20_000, "currentMa": 2_500, "powerMw": 50_000 },
                "heaterTelemetry": { "currentTempC": 94.0, "heaterOutputPercent": 30 },
                "status": { "heaterControlPhase": "approach" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "warmup",
                "elapsedMs": 2_000,
                "sourceTelemetry": { "voltageMv": 15_000, "currentMa": 1_800, "powerMw": 27_000 },
                "heaterTelemetry": { "currentTempC": 96.2, "heaterOutputPercent": 20 },
                "status": { "heaterControlPhase": "approach" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "hold",
                "elapsedMs": 3_000,
                "sourceTelemetry": { "voltageMv": 11_000, "currentMa": 1_600, "powerMw": 17_600 },
                "heaterTelemetry": { "currentTempC": 100.4, "heaterOutputPercent": 15 },
                "status": { "heaterControlPhase": "hold" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 100,
                "phase": "hold",
                "elapsedMs": 4_000,
                "sourceTelemetry": { "voltageMv": 10_000, "currentMa": 1_400, "powerMw": 14_000 },
                "heaterTelemetry": { "currentTempC": 103.8, "heaterOutputPercent": 0 },
                "status": { "heaterControlPhase": "hold" },
            }),
        ];

        let stage_samples = thermal_replay_stage_samples(&samples, 100).unwrap();
        let source_analysis = thermal_replay_stage_source_analysis(&stage_samples, 100);

        assert_eq!(source_analysis["approachSource"]["sampleCount"], json!(3));
        assert_eq!(
            source_analysis["approachSource"]["voltageMv"]["min"],
            json!(15_000.0)
        );
        assert_eq!(
            source_analysis["approachSource"]["voltageMv"]["max"],
            json!(21_000.0)
        );
        assert_eq!(
            source_analysis["approachSource"]["currentMa"]["last"],
            json!(1_800.0)
        );
        assert_eq!(source_analysis["holdSource"]["sampleCount"], json!(2));
        assert_eq!(
            source_analysis["holdSource"]["powerMw"]["first"],
            json!(17_600.0)
        );
        assert_eq!(
            source_analysis["holdSource"]["powerMw"]["last"],
            json!(14_000.0)
        );
    }

    #[test]
    fn thermal_replay_preserves_full_batch_candidate_profile() {
        let mut original = thermal_seed_candidate_profile();
        thermal_candidate_point_mut(&mut original, 60)
            .expect("60 point")
            .hold_power_permille = 777;
        let original_value = thermal_candidate_profile_to_value(&original);
        let heater_parameters =
            thermal_heater_parameters_value(140, Some(&original_value), "preview");
        let summary = json!({
            "candidateProfile": original_value,
            "parameters": { "seedProfileFile": null }
        });
        let samples = vec![json!({
            "testPhase": "applied",
            "targetTempC": 140,
            "heaterParameters": heater_parameters,
        })];

        let replayed = thermal_replay_applied_profile(&summary, &samples, &[140]).unwrap();

        assert_eq!(
            thermal_candidate_point(&replayed, 60)
                .expect("replayed 60 point")
                .hold_power_permille,
            777
        );
    }

    #[test]
    fn thermal_relation_rebuild_scales_power_against_default_curve() {
        let scale_power = |value: u16| ((f32::from(value) * 1.1) + 0.5).min(1_000.0) as u16;
        let lower_default = thermal_default_target_point(100);
        let upper_default = thermal_default_target_point(250);
        let target_default = thermal_default_target_point(140);
        let lower = ThermalCandidatePoint {
            target_temp_c: 100,
            brake_distance_centi_c: lower_default.brake_distance_centi_c.saturating_add(200),
            warmup_power_permille: scale_power(lower_default.warmup_power_permille),
            approach_power_permille: scale_power(lower_default.approach_power_permille),
            approach_floor_power_permille: scale_power(lower_default.approach_floor_power_permille),
            approach_damping_exponent_permille: lower_default
                .approach_damping_exponent_permille
                .saturating_add(150),
            approach_tail_window_centi_c: lower_default.approach_tail_window_centi_c,
            hold_power_permille: scale_power(lower_default.hold_power_permille),
            hold_reheat_power_permille: scale_power(lower_default.hold_reheat_power_permille),
            hold_entry_centi_c: lower_default.hold_entry_centi_c.saturating_add(5),
            hold_exit_centi_c: lower_default.hold_exit_centi_c.saturating_add(10),
            hold_on_centi_c: lower_default.hold_on_centi_c.saturating_add(5),
            hold_off_centi_c: lower_default.hold_off_centi_c.saturating_add(20),
            overshoot_cutoff_centi_c: lower_default.overshoot_cutoff_centi_c.saturating_add(30),
            hold_kp_permille_per_c: lower_default.hold_kp_permille_per_c.saturating_add(4),
            hold_ki_permille_per_c_tick: lower_default
                .hold_ki_permille_per_c_tick
                .saturating_add(1),
            hold_blend_ticks: lower_default.hold_blend_ticks.saturating_add(2),
            approach_lead_ticks: lower_default.approach_lead_ticks.saturating_add(3),
            hold_lead_ticks: lower_default.hold_lead_ticks.saturating_add(1),
        };
        let upper = ThermalCandidatePoint {
            target_temp_c: 250,
            brake_distance_centi_c: upper_default.brake_distance_centi_c.saturating_add(200),
            warmup_power_permille: scale_power(upper_default.warmup_power_permille),
            approach_power_permille: scale_power(upper_default.approach_power_permille),
            approach_floor_power_permille: scale_power(upper_default.approach_floor_power_permille),
            approach_damping_exponent_permille: upper_default
                .approach_damping_exponent_permille
                .saturating_add(150),
            approach_tail_window_centi_c: upper_default.approach_tail_window_centi_c,
            hold_power_permille: scale_power(upper_default.hold_power_permille),
            hold_reheat_power_permille: scale_power(upper_default.hold_reheat_power_permille),
            hold_entry_centi_c: upper_default.hold_entry_centi_c.saturating_add(5),
            hold_exit_centi_c: upper_default.hold_exit_centi_c.saturating_add(10),
            hold_on_centi_c: upper_default.hold_on_centi_c.saturating_add(5),
            hold_off_centi_c: upper_default.hold_off_centi_c.saturating_add(20),
            overshoot_cutoff_centi_c: upper_default.overshoot_cutoff_centi_c.saturating_add(30),
            hold_kp_permille_per_c: upper_default.hold_kp_permille_per_c.saturating_add(4),
            hold_ki_permille_per_c_tick: upper_default
                .hold_ki_permille_per_c_tick
                .saturating_add(1),
            hold_blend_ticks: upper_default.hold_blend_ticks.saturating_add(2),
            approach_lead_ticks: upper_default.approach_lead_ticks.saturating_add(3),
            hold_lead_ticks: upper_default.hold_lead_ticks.saturating_add(1),
        };

        let rebuilt = rebuild_thermal_candidate_point_from_anchor_relations(140, lower, upper);

        assert_eq!(
            rebuilt.hold_power_permille,
            scale_power(target_default.hold_power_permille)
        );
        assert_eq!(
            rebuilt.brake_distance_centi_c,
            target_default.brake_distance_centi_c.saturating_add(200)
        );
        assert_eq!(
            rebuilt.approach_damping_exponent_permille,
            target_default
                .approach_damping_exponent_permille
                .saturating_add(150)
        );
        assert!(rebuilt.approach_floor_power_permille >= rebuilt.hold_power_permille);
        assert!(rebuilt.approach_power_permille >= rebuilt.approach_floor_power_permille);
        assert!(rebuilt.warmup_power_permille >= rebuilt.approach_power_permille);
        assert!(rebuilt.hold_reheat_power_permille >= rebuilt.approach_floor_power_permille);
    }

    #[test]
    fn thermal_relation_rebuild_preserves_out_of_span_points() {
        let mut profile = thermal_seed_candidate_profile();
        if let Some(point) = thermal_candidate_point_mut(&mut profile, 220) {
            point.hold_power_permille = 777;
            point.hold_reheat_power_permille = 888;
        }
        if let Some(point) = thermal_candidate_point_mut(&mut profile, 250) {
            point.hold_power_permille = 901;
            point.hold_reheat_power_permille = 944;
        }

        thermal_rebuild_profile_from_anchor_targets(&mut profile, &[60, 100, 140, 180]);

        let point_220 = thermal_candidate_point(&profile, 220).expect("220 point");
        let point_250 = thermal_candidate_point(&profile, 250).expect("250 point");
        assert_eq!(point_220.hold_power_permille, 777);
        assert_eq!(point_220.hold_reheat_power_permille, 888);
        assert_eq!(point_250.hold_power_permille, 901);
        assert_eq!(point_250.hold_reheat_power_permille, 944);
        assert_eq!(point_220.target_temp_c, 220);
        assert_eq!(point_250.target_temp_c, 250);
    }

    #[test]
    fn thermal_persisted_profile_projects_bank_specific_targets() {
        let profile = thermal_candidate_profile_from_value(json!({
            "points": [
                {"targetTempC": 60, "brakeDistanceCentiC": 1100, "approachPowerPermille": 450, "approachFloorPowerPermille": 160, "approachDampingExponentPermille": 4000, "approachTailWindowCentiC": 375, "holdPowerPermille": 135, "holdReheatPowerPermille": 170, "holdEntryCentiC": 220, "holdExitCentiC": 400, "holdOnCentiC": 30, "holdOffCentiC": 40, "overshootCutoffCentiC": 50, "holdKpPermillePerC": 8, "holdKiPermillePerCTick": 1, "holdBlendTicks": 1, "approachLeadTicks": 6, "holdLeadTicks": 6, "warmupPowerPermille": 760},
                {"targetTempC": 80, "brakeDistanceCentiC": 980, "approachPowerPermille": 440, "approachFloorPowerPermille": 220, "approachDampingExponentPermille": 2610, "approachTailWindowCentiC": 375, "holdPowerPermille": 140, "holdReheatPowerPermille": 150, "holdEntryCentiC": 150, "holdExitCentiC": 230, "holdOnCentiC": 20, "holdOffCentiC": 115, "overshootCutoffCentiC": 140, "holdKpPermillePerC": 12, "holdKiPermillePerCTick": 1, "holdBlendTicks": 3, "approachLeadTicks": 7, "holdLeadTicks": 6, "warmupPowerPermille": 850},
                {"targetTempC": 100, "brakeDistanceCentiC": 1340, "approachPowerPermille": 340, "approachFloorPowerPermille": 220, "approachDampingExponentPermille": 1500, "approachTailWindowCentiC": 375, "holdPowerPermille": 220, "holdReheatPowerPermille": 220, "holdEntryCentiC": 150, "holdExitCentiC": 120, "holdOnCentiC": 10, "holdOffCentiC": 50, "overshootCutoffCentiC": 50, "holdKpPermillePerC": 20, "holdKiPermillePerCTick": 1, "holdBlendTicks": 2, "approachLeadTicks": 9, "holdLeadTicks": 8, "warmupPowerPermille": 1000},
                {"targetTempC": 140, "brakeDistanceCentiC": 940, "approachPowerPermille": 420, "approachFloorPowerPermille": 320, "approachDampingExponentPermille": 910, "approachTailWindowCentiC": 0, "holdPowerPermille": 335, "holdReheatPowerPermille": 400, "holdEntryCentiC": 150, "holdExitCentiC": 100, "holdOnCentiC": 10, "holdOffCentiC": 160, "overshootCutoffCentiC": 220, "holdKpPermillePerC": 22, "holdKiPermillePerCTick": 1, "holdBlendTicks": 1, "approachLeadTicks": 2, "holdLeadTicks": 0, "warmupPowerPermille": 1000},
                {"targetTempC": 180, "brakeDistanceCentiC": 650, "approachPowerPermille": 760, "approachFloorPowerPermille": 460, "approachDampingExponentPermille": 800, "approachTailWindowCentiC": 0, "holdPowerPermille": 450, "holdReheatPowerPermille": 620, "holdEntryCentiC": 120, "holdExitCentiC": 70, "holdOnCentiC": 25, "holdOffCentiC": 140, "overshootCutoffCentiC": 300, "holdKpPermillePerC": 20, "holdKiPermillePerCTick": 1, "holdBlendTicks": 3, "approachLeadTicks": 4, "holdLeadTicks": 0, "warmupPowerPermille": 1000},
                {"targetTempC": 220, "brakeDistanceCentiC": 400, "approachPowerPermille": 900, "approachFloorPowerPermille": 800, "approachDampingExponentPermille": 400, "approachTailWindowCentiC": 0, "holdPowerPermille": 750, "holdReheatPowerPermille": 850, "holdEntryCentiC": 120, "holdExitCentiC": 50, "holdOnCentiC": 5, "holdOffCentiC": 240, "overshootCutoffCentiC": 320, "holdKpPermillePerC": 22, "holdKiPermillePerCTick": 1, "holdBlendTicks": 2, "approachLeadTicks": 2, "holdLeadTicks": 0, "warmupPowerPermille": 1000},
                {"targetTempC": 250, "brakeDistanceCentiC": 200, "approachPowerPermille": 1000, "approachFloorPowerPermille": 1000, "approachDampingExponentPermille": 350, "approachTailWindowCentiC": 0, "holdPowerPermille": 850, "holdReheatPowerPermille": 930, "holdEntryCentiC": 150, "holdExitCentiC": 55, "holdOnCentiC": 14, "holdOffCentiC": 320, "overshootCutoffCentiC": 420, "holdKpPermillePerC": 12, "holdKiPermillePerCTick": 1, "holdBlendTicks": 1, "approachLeadTicks": 1, "holdLeadTicks": 0, "warmupPowerPermille": 1000}
            ],
            "settings": {}
        }));

        let pps3a = thermal_persisted_profile_for_bank(&profile, "pps3a");
        let pps5a = thermal_persisted_profile_for_bank(&profile, "pps5a");

        assert_eq!(
            pps3a
                .points
                .iter()
                .map(|point| point.target_temp_c)
                .collect::<Vec<_>>(),
            vec![60, 100, 140, 180, 220, 250]
        );
        assert_eq!(
            pps5a
                .points
                .iter()
                .map(|point| point.target_temp_c)
                .collect::<Vec<_>>(),
            vec![60, 100, 140, 180, 220, 240]
        );
        let point_240 = thermal_candidate_point(&pps5a, 240).expect("240 point");
        assert!(point_240.brake_distance_centi_c > 200);
        assert!(point_240.brake_distance_centi_c < 400);
    }

    #[test]
    fn parses_thermal_self_test_targets_subset_command() {
        let cli = Cli::try_parse_from([
            "flux-purr",
            "thermal",
            "self-test",
            "--device",
            "mock-fp-lab-01",
            "--source-id",
            "iso-mock",
            "--source-url",
            "http://127.0.0.1:1",
            "--targets-c",
            "140,250",
        ])
        .expect("parse thermal subset");

        let Command::Thermal {
            command: ThermalCommand::SelfTest(args),
        } = cli.command
        else {
            panic!("expected thermal self-test command");
        };

        assert_eq!(args.targets_c.as_deref(), Some("140,250"));
        assert_eq!(args.source_kind, BenchSourceKind::Isolapurr);
        assert_eq!(
            args.evaluation_mode,
            ThermalSelfTestEvaluationMode::HoldConfirm
        );
        assert_eq!(args.sample_interval_ms, 300);
        assert_eq!(effective_thermal_sample_interval_ms(333), 300);
    }

    #[test]
    fn parses_thermal_self_test_batch_candidate_files() {
        let cli = Cli::try_parse_from([
            "flux-purr",
            "thermal",
            "self-test",
            "--device",
            "mock-fp-lab-01",
            "--source-id",
            "iso-mock",
            "--source-url",
            "http://127.0.0.1:1",
            "--targets-c",
            "140",
            "--skip-optimize",
            "--candidate-profile-file",
            "/tmp/a.json",
            "--candidate-profile-file",
            "/tmp/b.json",
        ])
        .expect("parse thermal batch");

        let Command::Thermal {
            command: ThermalCommand::SelfTest(args),
        } = cli.command
        else {
            panic!("expected thermal self-test command");
        };

        assert_eq!(
            args.candidate_profile_files,
            vec![PathBuf::from("/tmp/a.json"), PathBuf::from("/tmp/b.json")]
        );
    }

    #[test]
    fn thermal_batch_restart_uses_target_minus_thirty_with_forty_degree_floor() {
        assert_eq!(thermal_batch_restart_temp_c(60, 40.0), 40.0);
        assert_eq!(thermal_batch_restart_temp_c(100, 40.0), 70.0);
        assert_eq!(thermal_batch_restart_temp_c(220, 40.0), 190.0);
    }

    #[test]
    fn thermal_batch_restart_respects_explicit_cooldown_override() {
        assert_eq!(thermal_batch_restart_temp_c(60, 35.0), 35.0);
        assert_eq!(thermal_batch_restart_temp_c(220, 180.0), 180.0);
    }

    #[test]
    fn parses_thermal_retune_command() {
        let cli = Cli::try_parse_from([
            "flux-purr",
            "thermal",
            "retune",
            "--run-dir",
            "/tmp/thermal-run",
            "--optimize-targets-c",
            "100,220",
        ])
        .expect("parse thermal retune");

        let Command::Thermal {
            command: ThermalCommand::Retune(args),
        } = cli.command
        else {
            panic!("expected thermal retune command");
        };

        assert_eq!(args.run_dir, PathBuf::from("/tmp/thermal-run"));
        assert_eq!(args.optimize_targets_c.as_deref(), Some("100,220"));
    }

    #[test]
    fn parses_thermal_report_rerender_legacy_command() {
        let cli = Cli::try_parse_from([
            "flux-purr",
            "thermal",
            "report",
            "rerender-legacy",
            "--legacy-bundle-dir",
            "/tmp/legacy-bundle",
            "--output-dir",
            "/tmp/compliant-bundle",
        ])
        .expect("parse thermal report rerender legacy");

        let Command::Thermal {
            command: ThermalCommand::Report { command },
        } = cli.command
        else {
            panic!("expected thermal report command");
        };

        let ThermalReportCommand::RerenderLegacy(args) = command;

        assert_eq!(args.legacy_bundle_dir, PathBuf::from("/tmp/legacy-bundle"));
        assert_eq!(args.output_dir, Some(PathBuf::from("/tmp/compliant-bundle")));
    }

    #[test]
    fn thermal_hold_tracker_samples_one_minute_after_entering_hold() {
        let start = tokio::time::Instant::now();
        let mut tracker = ThermalHoldTracker::new(120, Duration::from_secs(10));

        assert_eq!(
            tracker.observe(119.6, 1_000, start, false),
            ThermalHoldObservation::Warmup
        );
        assert_eq!(
            tracker.observe(119.6, 1_500, start + Duration::from_millis(500), true),
            ThermalHoldObservation::Hold
        );
        assert_eq!(
            tracker.observe(119.8, 5_000, start + Duration::from_secs(5), true),
            ThermalHoldObservation::Hold
        );
        assert_eq!(
            tracker.observe(119.2, 6_000, start + Duration::from_secs(6), false),
            ThermalHoldObservation::Hold
        );
        assert_eq!(tracker.rise_time_ms(), Some(1_500));
        assert_eq!(
            tracker.observe(119.7, 11_500, start + Duration::from_millis(11_500), false),
            ThermalHoldObservation::Completed
        );
        assert!((tracker.peak_to_peak_c() - 0.6).abs() < 0.001);
    }

    #[test]
    fn thermal_approach_guard_fails_when_hold_threshold_is_not_crossed_in_ten_seconds() {
        let mut guard = ThermalApproachGuardTracker::new(140, 20);

        assert_eq!(guard.observe(132.0, 0, Some("approach")), None);
        assert_eq!(guard.observe(137.9, 10_000, Some("approach")), None);
        assert_eq!(
            guard.observe(137.9, 10_001, Some("approach")),
            Some("approach_threshold_timeout")
        );

        let analysis = guard.finalize();
        assert_eq!(analysis.approach_started_at_ms, Some(0));
        assert_eq!(analysis.hold_threshold_crossed_at_ms, None);
        assert_eq!(analysis.first_hold_at_ms, None);
    }

    #[test]
    fn thermal_approach_guard_fails_when_approach_reenters_warmup_before_hold() {
        let mut guard = ThermalApproachGuardTracker::new(180, 15);

        assert_eq!(guard.observe(170.0, 1_000, Some("approach")), None);
        assert_eq!(
            guard.observe(165.0, 4_000, Some("warmup")),
            Some("approach_reentered_warmup")
        );

        let analysis = guard.finalize();
        assert_eq!(analysis.approach_started_at_ms, Some(1_000));
        assert_eq!(analysis.warmup_reentered_at_ms, Some(4_000));
        assert_eq!(analysis.first_hold_at_ms, None);
    }

    #[test]
    fn thermal_approach_guard_fails_when_hold_is_not_entered_in_thirty_seconds() {
        let mut guard = ThermalApproachGuardTracker::new(220, 8);

        assert_eq!(guard.observe(215.0, 500, Some("approach")), None);
        assert_eq!(guard.observe(219.95, 8_000, Some("approach")), None);
        assert_eq!(guard.observe(219.95, 30_000, Some("approach")), None);
        assert_eq!(
            guard.observe(219.95, 30_501, Some("approach")),
            Some("approach_hold_timeout")
        );

        let analysis = guard.finalize();
        assert_eq!(analysis.approach_started_at_ms, Some(500));
        assert_eq!(analysis.hold_threshold_crossed_at_ms, Some(8_000));
        assert_eq!(analysis.first_hold_at_ms, None);
    }

    #[test]
    fn thermal_full_speed_tracker_accepts_hold_entry_within_ten_seconds() {
        let mut tracker = ThermalFullSpeedStableTracker::new(140);

        assert_eq!(
            tracker.observe(100.0, 0, Some("warmup")),
            ThermalFullSpeedStableObservation::Pending
        );
        assert_eq!(
            tracker.observe(132.0, 250, Some("approach")),
            ThermalFullSpeedStableObservation::Pending
        );
        assert_eq!(
            tracker.observe(139.2, 2_000, Some("hold")),
            ThermalFullSpeedStableObservation::Pending
        );
        assert_eq!(
            tracker.observe(140.0, 11_000, Some("hold")),
            ThermalFullSpeedStableObservation::Pending
        );
        assert_eq!(
            tracker.observe(140.0, 12_000, Some("approach")),
            ThermalFullSpeedStableObservation::Verified
        );
        assert_eq!(
            tracker.observe(139.0, 12_250, Some("approach")),
            ThermalFullSpeedStableObservation::Verified
        );
        assert_eq!(tracker.finalize().settle_time_ms, Some(1_750));

        let analysis = tracker.finalize();
        assert_eq!(analysis.warmup_exited_at_ms, Some(250));
        assert_eq!(analysis.stable_window_started_at_ms, Some(2_000));
        assert_eq!(analysis.stable_window_verified_at_ms, Some(12_000));
        assert_eq!(analysis.settle_time_ms, Some(1_750));
    }

    #[test]
    fn thermal_full_speed_tracker_keeps_window_across_active_control_phases() {
        let mut tracker = ThermalFullSpeedStableTracker::new(180);

        assert_eq!(
            tracker.observe(173.0, 0, Some("warmup")),
            ThermalFullSpeedStableObservation::Pending
        );
        assert_eq!(
            tracker.observe(178.8, 1_000, Some("approach")),
            ThermalFullSpeedStableObservation::Pending
        );
        assert_eq!(
            tracker.observe(180.4, 6_000, Some("hold")),
            ThermalFullSpeedStableObservation::Pending
        );
        assert_eq!(
            tracker.observe(179.2, 11_000, Some("approach")),
            ThermalFullSpeedStableObservation::Verified
        );

        let analysis = tracker.finalize();
        assert_eq!(analysis.stable_window_started_at_ms, Some(1_000));
        assert_eq!(analysis.stable_window_verified_at_ms, Some(11_000));
        assert_eq!(analysis.settle_time_ms, Some(0));
    }

    #[test]
    fn thermal_full_speed_tracker_fails_once_a_timely_window_is_impossible() {
        let mut tracker = ThermalFullSpeedStableTracker::new(140);

        assert_eq!(
            tracker.observe(100.0, 0, Some("warmup")),
            ThermalFullSpeedStableObservation::Pending
        );
        assert_eq!(
            tracker.observe(132.0, 250, Some("approach")),
            ThermalFullSpeedStableObservation::Pending
        );
        assert_eq!(
            tracker.observe(138.0, 10_251, Some("approach")),
            ThermalFullSpeedStableObservation::Failed("full_speed_to_stable_timeout")
        );
        assert_eq!(
            tracker.finalize().failure_reason,
            Some("full_speed_to_stable_timeout")
        );
    }

    #[test]
    fn thermal_replay_analysis_includes_post_hold_approach_samples_in_hold_window() {
        let samples = vec![
            json!({
                "testPhase": "applied",
                "targetTempC": 220,
                "phase": "warmup",
                "elapsedMs": 0,
                "heaterTelemetry": { "currentTempC": 216.2, "heaterOutputPercent": 91 },
                "status": { "heaterControlPhase": "approach" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 220,
                "phase": "hold",
                "elapsedMs": 1_000,
                "heaterTelemetry": { "currentTempC": 220.3, "heaterOutputPercent": 74 },
                "status": { "heaterControlPhase": "hold" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 220,
                "phase": "hold",
                "elapsedMs": 2_000,
                "heaterTelemetry": { "currentTempC": 221.0, "heaterOutputPercent": 72 },
                "status": { "heaterControlPhase": "hold" },
            }),
            json!({
                "testPhase": "applied",
                "targetTempC": 220,
                "phase": "hold",
                "elapsedMs": 3_000,
                "heaterTelemetry": { "currentTempC": 218.2, "heaterOutputPercent": 86 },
                "status": { "heaterControlPhase": "approach" },
            }),
        ];

        let stage_samples = thermal_replay_stage_samples(&samples, 220).unwrap();
        let analysis = thermal_replay_stage_analysis(&stage_samples, 220);

        assert_eq!(analysis.first_hold_temp_c, Some(220.3));
        assert_eq!(analysis.hold_sample_count, 3);
        assert_eq!(analysis.hold_median_output_permille, Some(740));
        assert_eq!(analysis.hold_max_above_target_c, Some(1.0));
        assert!((analysis.hold_max_below_target_c.unwrap_or_default() - 1.8).abs() < 0.001);
    }

    #[test]
    fn thermal_curve_oscillation_class_triggers_hold_ripple_tuning_even_below_legacy_p2p_limit() {
        let previous = ThermalCandidatePoint {
            target_temp_c: 100,
            brake_distance_centi_c: 1_000,
            warmup_power_permille: 1_000,
            approach_power_permille: 240,
            approach_floor_power_permille: 130,
            approach_damping_exponent_permille: 1_000,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 60,
            hold_reheat_power_permille: 120,
            hold_entry_centi_c: 30,
            hold_exit_centi_c: 90,
            hold_on_centi_c: 30,
            hold_off_centi_c: 160,
            overshoot_cutoff_centi_c: 200,
            hold_kp_permille_per_c: 10,
            hold_ki_permille_per_c_tick: 2,
            hold_blend_ticks: 8,
            approach_lead_ticks: 4,
            hold_lead_ticks: 0,
        };

        let tuned = tune_thermal_candidate_point(
            previous,
            &ThermalStageResult {
                target_temp_c: 100,
                rise_time_ms: 8_200,
                max_overshoot_c: 1.0,
                hold_peak_to_peak_c: 2.0,
                sample_count: 140,
                stop_reason: "completed",
                terminal_runtime_drop_reason: None,
                analysis: ThermalStageAnalysis {
                    first_hold_error_c: Some(0.1),
                    hold_median_output_permille: Some(120),
                    hold_p90_output_permille: Some(200),
                    hold_mean_error_c: Some(0.2),
                    hold_max_above_target_c: Some(0.8),
                    hold_max_below_target_c: Some(1.2),
                    approach_curve_deviation_class: Some("oscillatory_near_target"),
                    approach_curve_max_above_c: Some(0.8),
                    approach_curve_max_below_c: Some(1.2),
                    hold_sample_count: 40,
                    ..ThermalStageAnalysis::default()
                },
                guard: ThermalApproachGuardAnalysis::default(),
                full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
            },
        );

        assert_eq!(tuned.hold_power_permille, 120);
        assert_eq!(tuned.approach_floor_power_permille, 140);
        assert_eq!(tuned.hold_reheat_power_permille, 200);
        assert_eq!(tuned.hold_kp_permille_per_c, 13);
    }

    #[test]
    fn step_toward_u16_tolerates_reversed_bounds() {
        assert_eq!(step_toward_u16(1_000, 1_020, 40, 1_020, 1_000), 1_020);
        assert_eq!(step_toward_u16(980, 1_020, 40, 1_020, 1_000), 1_020);
    }

    #[test]
    fn thermal_self_test_detects_runtime_drop_and_disarmed_heater() {
        let running = json!({
            "mode": "sampling",
            "uptimeSeconds": 34,
            "targetTempC": 210,
            "heaterEnabled": true,
        });
        assert_eq!(thermal_runtime_drop_reason(&running, 210, Some(33)), None);

        let latched_fault = json!({
            "mode": "sampling",
            "uptimeSeconds": 34,
            "targetTempC": 210,
            "heaterEnabled": true,
            "heaterFaultReason": "sensor-open",
        });
        assert_eq!(
            thermal_runtime_drop_reason(&latched_fault, 210, Some(33)),
            Some(ThermalRuntimeDropReason::LatchedFault)
        );
        assert!(thermal_recoverable_sensor_fault(&latched_fault));

        let glitch = json!({
            "mode": "sampling",
            "uptimeSeconds": 34,
            "targetTempC": 210,
            "heaterEnabled": true,
            "heaterFaultReason": "sensor-glitch",
        });
        assert_eq!(
            thermal_runtime_drop_reason(&glitch, 210, Some(33)),
            Some(ThermalRuntimeDropReason::LatchedFault)
        );
        assert!(thermal_recoverable_sensor_fault(&glitch));

        let active_fault = json!({
            "mode": "fault",
            "heaterFaultReason": "sensor-open",
        });
        assert!(!thermal_recoverable_sensor_fault(&active_fault));

        let over_temp = json!({
            "mode": "idle",
            "heaterFaultReason": "over-temp",
        });
        assert!(!thermal_recoverable_sensor_fault(&over_temp));

        let reset = json!({
            "mode": "idle",
            "uptimeSeconds": 0,
            "targetTempC": 210,
            "heaterEnabled": false,
        });
        assert_eq!(
            thermal_runtime_drop_reason(&reset, 210, Some(34)),
            Some(ThermalRuntimeDropReason::UptimeReset)
        );

        let idle = json!({
            "mode": "idle",
            "uptimeSeconds": 35,
            "targetTempC": 210,
            "heaterEnabled": false,
        });
        assert_eq!(
            thermal_runtime_drop_reason(&idle, 210, Some(34)),
            Some(ThermalRuntimeDropReason::WrongMode)
        );

        let disarmed = json!({
            "mode": "sampling",
            "uptimeSeconds": 35,
            "targetTempC": 210,
            "heaterEnabled": false,
        });
        assert_eq!(
            thermal_runtime_drop_reason(&disarmed, 210, Some(34)),
            Some(ThermalRuntimeDropReason::HeaterDisarmed)
        );
    }

    #[test]
    fn thermal_self_test_shutdown_body_enables_active_cooling() {
        assert_eq!(
            thermal_self_test_runtime_body(false, 220),
            json!({
                "heaterEnabled": false,
                "targetTempC": 220,
                "activeCoolingEnabled": true,
            })
        );
        assert_eq!(
            thermal_self_test_runtime_body(true, 220),
            json!({
                "heaterEnabled": true,
                "targetTempC": 220,
            })
        );
    }

    #[test]
    fn thermal_self_test_cooldown_body_clears_preview_and_enables_active_cooling() {
        assert_eq!(
            thermal_self_test_cooldown_runtime_body(),
            json!({
                "heaterEnabled": false,
                "activeCoolingEnabled": true,
                "thermalControlProfile": {
                    "op": "clear_preview"
                }
            })
        );
    }

    #[test]
    fn isolapurr_power_config_matchers_validate_manual_target_and_auto_mode() {
        let manual_config = json!({
            "tpsMode": "manual",
            "manual": {
                "voltageMv": 20_000,
                "currentLimitMa": 3_250,
                "usbCPathMode": "force",
                "pathPolicy": "force_open",
            }
        });
        let auto_config = json!({
            "tpsMode": "autoFollow",
            "manual": {
                "voltageMv": 20_000,
                "currentLimitMa": 3_250,
                "usbCPathMode": "default",
                "pathPolicy": "auto",
            }
        });

        assert!(isolapurr_power_config_value_matches_manual(
            &manual_config,
            20_000,
            3_250
        ));
        assert!(!isolapurr_power_config_value_is_auto(&manual_config));
        assert!(isolapurr_power_config_value_is_auto(&auto_config));
        assert!(isolapurr_power_config_path_is_automatic(&auto_config));
        assert!(!isolapurr_power_config_value_matches_manual(
            &manual_config,
            20_000,
            3_000
        ));

        let capped_100w_config = json!({
            "tpsMode": "manual",
            "manual": {
                "voltageMv": 21_000,
                "currentLimitMa": 4_750,
                "usbCPathMode": "force",
                "pathPolicy": "force_open",
            },
            "capability": {
                "powerWatts": 100,
                "protocols": { "pd": true },
                "pd": { "pps": true, "fixedVoltagesMv": [9000, 12000, 15000, 20000] },
                "current": { "pps3LimitMa": 5_000, "pdPps5a": true }
            }
        });
        assert!(isolapurr_power_config_value_matches_manual(
            &capped_100w_config,
            21_000,
            5_000
        ));
    }

    #[test]
    fn parses_isolapurr_power_show_nested_usb_c_telemetry() {
        let power_show = json!({
            "ports": {
                "ports": [{
                    "portId": "port_c",
                    "label": "USB-C",
                    "telemetry": {
                        "status": "ok",
                        "voltage_mv": 20_010,
                        "current_ma": 1_240,
                        "power_mw": 24_812,
                        "sample_uptime_ms": 99,
                    }
                }]
            }
        });

        let telemetry = parse_isolapurr_live_telemetry(&power_show).unwrap();
        assert_eq!(telemetry.voltage_mv, 20_010);
        assert_eq!(telemetry.current_ma, 1_240);
        assert_eq!(telemetry.power_mw, 24_812);
    }

    #[test]
    fn validates_isolapurr_thermal_capability_and_ready_voltage() {
        let config = json!({
            "capability": {
                "power_watts": 65,
                "protocols": { "pd": true },
                "pd": {
                    "pps": true,
                    "fixed_voltages_mv": [9000, 12000, 15000, 20000]
                }
            }
        });
        assert!(isolapurr_power_config_has_thermal_capability(
            &config,
            THERMAL_SOURCE_65W_POWER_WATTS,
            false,
        ));
        assert!(!isolapurr_power_config_has_thermal_capability(
            &config,
            THERMAL_SOURCE_100W_POWER_WATTS,
            true,
        ));

        let five_amp_config = json!({
            "capability": {
                "powerWatts": 100,
                "protocols": { "pd": true },
                "pd": {
                    "pps": true,
                    "fixedVoltagesMv": [9000, 12000, 15000, 20000, 21000],
                    "pps3LimitMa": 5000,
                    "pdPps5a": true,
                }
            }
        });
        assert!(isolapurr_power_config_has_thermal_capability(
            &five_amp_config,
            THERMAL_SOURCE_100W_POWER_WATTS,
            true,
        ));
        let released_five_amp_config = json!({
            "capability": {
                "power_watts": 100,
                "protocols": { "pd": true },
                "pd": {
                    "pps": true,
                    "fixed_voltages_mv": [9000, 12000, 15000, 20000]
                },
                "current": {
                    "pps3_limit_ma": 5000,
                    "pd_pps_5a": true
                }
            }
        });
        assert!(isolapurr_power_config_has_thermal_capability(
            &released_five_amp_config,
            THERMAL_SOURCE_100W_POWER_WATTS,
            true,
        ));

        let ready = BenchSourceLiveTelemetry {
            voltage_mv: 12_034,
            current_ma: 119,
            power_mw: 1_431,
            sample_uptime_ms: 136_324,
            status: "ok".into(),
        };
        assert!(validate_isolapurr_ready_voltage(&ready).is_ok());

        let undervoltage = BenchSourceLiveTelemetry {
            voltage_mv: 5_000,
            ..ready.clone()
        };
        assert!(validate_isolapurr_ready_voltage(&undervoltage).is_err());

        let disconnected = BenchSourceLiveTelemetry {
            status: "not_inserted".into(),
            ..ready
        };
        assert!(validate_isolapurr_ready_voltage(&disconnected).is_err());
    }

    #[test]
    fn isolapurr_configured_thermal_source_class_detects_3a_and_5a_modes() {
        let three_amp_config = json!({
            "capability": {
                "power_watts": 65,
                "protocols": { "pd": true },
                "pd": {
                    "pps": true,
                    "fixed_voltages_mv": [9000, 12000, 15000, 20000]
                },
                "current": {
                    "pps3_limit_ma": 3250
                }
            }
        });
        assert_eq!(
            isolapurr_configured_thermal_source_class(&three_amp_config),
            Some("pps3a")
        );

        let five_amp_config = json!({
            "capability": {
                "power_watts": 100,
                "protocols": { "pd": true },
                "pd": {
                    "pps": true,
                    "fixed_voltages_mv": [9000, 12000, 15000, 20000]
                },
                "current": {
                    "pps3_limit_ma": 5000
                }
            }
        });
        assert_eq!(
            isolapurr_configured_thermal_source_class(&five_amp_config),
            Some("pps5a")
        );

        let missing_20v = json!({
            "capability": {
                "protocols": { "pd": true },
                "pd": {
                    "pps": true,
                    "fixed_voltages_mv": [9000, 12000, 15000]
                },
                "current": {
                    "pps3_limit_ma": 5000
                }
            }
        });
        assert_eq!(
            isolapurr_configured_thermal_source_class(&missing_20v),
            None
        );
    }

    #[test]
    fn isolapurr_write_response_requires_positive_acknowledgement() {
        assert!(isolapurr_cli_write_succeeded(&json!({"accepted": true})));
        assert!(isolapurr_cli_write_succeeded(&json!({"ok": true})));
        assert!(!isolapurr_cli_write_succeeded(&json!({})));
        assert!(!isolapurr_cli_write_succeeded(&json!({"accepted": false})));
    }

    #[test]
    fn isolapurr_status_identity_must_match_requested_device() {
        let status = json!({
            "device": {
                "device_id": "f293cc9c139e",
                "firmware": {
                    "name": "isolapurr-usb-hub",
                    "version": "0.5.1"
                }
            }
        });

        assert!(isolapurr_status_identity_matches(&status, "f293cc9c139e"));
        assert!(!isolapurr_status_identity_matches(&status, "856a141cdbd4"));

        let wrapped_status = json!({
            "ok": true,
            "result": {
                "device": {
                    "device_id": "f293cc9c139e"
                }
            }
        });
        assert!(isolapurr_status_identity_matches(
            &wrapped_status,
            "f293cc9c139e"
        ));
        assert!(!isolapurr_status_identity_matches(
            &wrapped_status,
            "856a141cdbd4"
        ));
    }

    #[test]
    fn parses_thermal_self_test_command() {
        let cli = Cli::try_parse_from([
            "flux-purr",
            "--devd",
            DEFAULT_DEVD_URL,
            "thermal",
            "self-test",
            "--device",
            "bench",
            "--source-device-id",
            "iso-1",
            "--source-url",
            "http://192.168.31.122",
            "--dry-run",
            "--json",
        ])
        .unwrap();

        match cli.command {
            Command::Thermal {
                command: ThermalCommand::SelfTest(args),
            } => {
                assert_eq!(args.target.device.as_deref(), Some("bench"));
                assert_eq!(args.source_kind, BenchSourceKind::Isolapurr);
                assert_eq!(args.source_id, "iso-1");
                assert_eq!(args.source_url, "http://192.168.31.122");
                assert_eq!(args.source_mode, "auto-follow");
                assert!(args.dry_run);
            }
            other => panic!("unexpected command parsed: {other:?}"),
        }
    }

    #[test]
    fn parses_pps_volts_as_100mv_steps() {
        assert_eq!(parse_pps_volts("10.4").unwrap(), 10_400);
        assert_eq!(parse_pps_volts("21").unwrap(), 21_000);
        assert_eq!(parse_pps_volts("28").unwrap(), 28_000);
        assert!(parse_pps_volts("10.45").is_err());
        assert!(parse_pps_volts("4.9").is_err());
        assert!(parse_pps_volts("28.1").is_err());
    }

    #[test]
    fn parses_pps_amps_as_50ma_steps() {
        assert_eq!(parse_pps_amps("2.5").unwrap(), 2_500);
        assert_eq!(parse_pps_amps("3.00").unwrap(), 3_000);
        assert!(parse_pps_amps("2.53").is_err());
        assert!(parse_pps_amps("0").is_err());
    }

    #[test]
    fn thermal_profile_modes_preserve_65w_and_define_100w_defaults() {
        assert_eq!(
            ThermalProfileMode::W65.explicit_source_defaults(),
            Some((20_000, 3_250))
        );
        assert_eq!(
            ThermalProfileMode::W100.explicit_source_defaults(),
            Some((21_000, 5_000))
        );
        assert_eq!(ThermalProfileMode::Auto.explicit_bank(), None);
        assert_eq!(ThermalProfileMode::W100.explicit_bank(), Some("pps5a"));
    }

    #[test]
    fn thermal_default_seed_candidates_follow_bank_current_truth() {
        let (_, pps3a_seed_path) = load_thermal_default_seed_candidate_profile("pps3a").unwrap();
        let (_, pps5a_seed_path) = load_thermal_default_seed_candidate_profile("pps5a").unwrap();

        assert!(
            pps3a_seed_path
                .as_ref()
                .is_some_and(|path| path.ends_with(THERMAL_PPS3A_ACCEPTED_SEED_RELATIVE))
        );
        assert!(pps5a_seed_path.as_ref().is_some_and(|path| {
            path.ends_with(THERMAL_PPS5A_ACCEPTED_SEED_RELATIVE)
                || path.ends_with(THERMAL_PPS5A_TUNING_SEED_RELATIVE)
        }));
    }

    #[test]
    fn thermal_profile_preview_body_preserves_selected_mode() {
        let body = thermal_profile_preview_runtime_body(
            ThermalProfileMode::W100,
            json!({"points": [], "settings": {}}),
        );

        assert_eq!(body["thermalProfileMode"], "100w");
        assert_eq!(body["thermalControlProfile"]["op"], "preview");
        assert!(body["thermalControlProfile"]["profile"].is_object());
    }

    #[test]
    fn thermal_profile_preview_readback_requires_the_requested_bank() {
        let matched = json!({
            "thermalProfileMode": "100w",
            "thermalProfileResolvedBank": "pps5a",
        });
        assert!(verify_thermal_profile_mode_readback(&matched, ThermalProfileMode::W100).is_ok());

        let wrong_bank = json!({
            "thermalProfileMode": "100w",
            "thermalProfileResolvedBank": "pps3a",
        });
        assert!(
            verify_thermal_profile_mode_readback(&wrong_bank, ThermalProfileMode::W100).is_err()
        );
    }

    #[test]
    fn thermal_source_class_uses_the_configured_capability_not_selected_mode() {
        assert_eq!(thermal_source_class(20_000, 3_250), "pps3a");
        assert_eq!(thermal_source_class(21_000, 5_000), "pps5a");
    }

    #[test]
    fn calibration_heater_commands_accept_explicit_boolean_values() {
        let cli = Cli::try_parse_from([
            "flux-purr",
            "--devd",
            DEFAULT_DEVD_URL,
            "calibration-mode",
            "temperature",
            "heater",
            "--enabled",
            "false",
            "--device",
            "bench",
            "--json",
        ])
        .unwrap();

        match cli.command {
            Command::CalibrationMode {
                command:
                    CalibrationModeCommand::Temperature {
                        command: TemperatureCalibrationCommand::Heater(args),
                    },
            } => assert!(!args.enabled),
            other => panic!("unexpected command parsed: {other:?}"),
        }
    }

    #[test]
    fn parses_single_artifact_manifest() {
        let artifact = FirmwareArtifact {
            artifact_id: "a".to_string(),
            name: "A".to_string(),
            version: "v".to_string(),
            git_sha: "sha".to_string(),
            build_id: "build".to_string(),
            target_chip: "esp32s3".to_string(),
            profile: "release".to_string(),
            features: vec!["web_serial".to_string()],
            protocol: "flux-purr.usb.v1".to_string(),
            files: Vec::new(),
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("artifact.json");
        fs::write(&path, serde_json::to_vec(&artifact).unwrap()).unwrap();
        let artifacts = read_artifact_manifest(&path).unwrap();
        assert_eq!(artifacts[0].artifact_id, "a");
    }

    fn write_retune_fixture(run_dir: &Path) {
        fs::create_dir_all(run_dir).unwrap();
        let samples_path = run_dir.join("samples.ndjson");
        let profile = default_thermal_candidate_profile();
        let mut samples_writer = BufWriter::new(File::create(&samples_path).unwrap());
        let mut sample_index = 0usize;
        let applied = write_dry_thermal_ladder(
            &mut samples_writer,
            "thermal-fixture",
            "applied",
            20_000,
            3_250,
            Some(&profile),
            "preview",
            &[60],
            &mut sample_index,
        )
        .unwrap();
        samples_writer.flush().unwrap();
        let summary = json!({
            "kind": "thermal_self_test",
            "ok": true,
            "runId": "thermal-fixture",
            "dryRun": true,
            "target": {
                "deviceId": "bench",
                "hardwareId": Value::Null,
                "devd": DEFAULT_DEVD_URL,
            },
            "source": {
                "deviceId": "iso-fixture",
                "mode": "dry_run",
                "url": "http://127.0.0.1:1",
            },
            "parameters": {
                "targetsC": [60],
                "optimizeTargetsC": [],
                "sampleIntervalMs": 300,
                "effectiveSampleIntervalMs": 300,
                "holdSeconds": 60,
                "stageTimeoutSeconds": 300,
                "runtimeRearmAttempts": 1,
                "cooldownTempC": 40.0,
                "cooldownTimeoutSeconds": 7200,
                "limits": {
                    "overshootC": 3.0,
                    "holdPeakToPeakC": 3.0,
                    "fullSpeedToStableMs": {
                        "lte150C": ThermalFullSpeedStableTracker::LOW_TEMP_SETTLE_LIMIT_MS,
                        "gt150C": ThermalFullSpeedStableTracker::HIGH_TEMP_SETTLE_LIMIT_MS,
                    },
                },
                "seedProfileFile": Value::Null,
            },
            "files": {
                "runDir": run_dir,
                "summaryPath": run_dir.join("run.json"),
                "samplesPath": samples_path,
                "candidateProfilePath": run_dir.join("thermal-profile.candidate.json"),
            },
            "candidateProfile": profile,
            "profilePersistence": "dry_run",
            "tuningSteps": [],
            "applied": applied.iter().map(ThermalStageResult::to_value).collect::<Vec<_>>(),
            "validation": validate_thermal_applied_results(
                &applied,
                &[60],
                ThermalSelfTestEvaluationMode::HoldConfirm,
            ),
            "sampleCount": sample_index,
            "complete": true,
            "error": Value::Null,
        });
        fs::write(
            run_dir.join("run.json"),
            serde_json::to_vec_pretty(&summary).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn thermal_retune_offline_writes_replay_artifacts_without_apply_receipt() {
        let dir = tempfile::tempdir().unwrap();
        write_retune_fixture(dir.path());

        let output =
            thermal_retune::retune_thermal_self_test_run(thermal_retune::ThermalRetuneInput {
                run_dir: dir.path().to_path_buf(),
                optimize_targets_c: None,
            })
            .unwrap();

        assert_eq!(
            output.summary["kind"].as_str(),
            Some("thermal_self_test_replay")
        );
        assert!(output.summary.get("applyPreview").is_none());
        assert!(dir.path().join("run.replayed.json").exists());
        assert!(
            dir.path()
                .join("thermal-profile.replayed.candidate.json")
                .exists()
        );
    }

    fn thermal_report_test_point(target_temp_c: i16) -> Value {
        json!({
            "targetTempC": target_temp_c,
            "brakeDistanceCentiC": 1100,
            "warmupPowerPermille": 1000,
            "approachPowerPermille": 420,
            "approachFloorPowerPermille": 320,
            "approachDampingExponentPermille": 910,
            "approachTailWindowCentiC": 0,
            "holdPowerPermille": 335,
            "holdReheatPowerPermille": 400,
            "holdEntryCentiC": 150,
            "holdExitCentiC": 100,
            "holdOnCentiC": 10,
            "holdOffCentiC": 160,
            "overshootCutoffCentiC": 220,
            "holdKpPermillePerC": 22,
            "holdKiPermillePerCTick": 1,
            "holdBlendTicks": 1,
            "approachLeadTicks": 2,
            "holdLeadTicks": 0
        })
    }

    #[test]
    fn thermal_report_rerender_preliminary_bundle_writes_compliant_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join("legacy");
        let output_dir = dir.path().join("rerendered");
        fs::create_dir_all(&legacy_dir).unwrap();

        let legacy_bundle = json!({
            "kind": "thermal_approach_characterization",
            "runId": "legacy-run",
            "generatedAt": "2026-07-17T12:00:00Z",
            "selectedMode": "100w",
            "resolvedBank": "pps5a",
            "detectedSourceClass": "pps5a",
            "bundleDisposition": "preliminary_review",
            "acceptedProfileRole": "review_candidate_snapshot",
            "source": {
                "sourceDeviceId": "f293cc9c139e"
            },
            "targets": [
                {
                    "targetTempC": 60,
                    "effectivePoint": thermal_report_test_point(60),
                    "variants": [
                        {
                            "variantId": "zero_coast",
                            "variantLabel": "0加热",
                            "valid": true,
                            "tunedPoint": thermal_report_test_point(60),
                            "metrics": {
                                "approachDurationMs": 8200,
                                "peak": 0.75,
                                "rollback": 1.71
                            },
                            "samples": [
                                {
                                    "elapsedMs": 0,
                                    "currentTempC": 35.0,
                                    "heaterFilteredTempC": 35.0,
                                    "heaterControlPhase": "warmup",
                                    "heaterOutputPercent": 100,
                                    "heaterPhysicalOutputPercent": 100,
                                    "sourceTelemetry": {
                                        "voltageMv": 21000,
                                        "currentMa": 4800,
                                        "powerMw": 100800
                                    }
                                }
                            ]
                        }
                    ],
                    "holdCheck": {
                        "confirmRunId": "confirm-60",
                        "passed": true,
                        "failureReason": Value::Null,
                        "holdSeconds": 60,
                        "maxOvershootC": 0.75,
                        "holdPeakToPeakC": 1.71,
                        "holdMedianOutputPermille": 0,
                        "holdP90OutputPermille": 100,
                        "approachSource": {"powerMw": {"avg": 22425.0}},
                        "holdSource": {"powerMw": {"avg": 3935.0}},
                        "stopReason": "completed"
                    }
                }
            ]
        });
        fs::write(
            legacy_dir.join("run.bundle.json"),
            serde_json::to_vec_pretty(&legacy_bundle).unwrap(),
        )
        .unwrap();
        fs::write(
            legacy_dir.join("thermal-profile.accepted.json"),
            serde_json::to_vec_pretty(&json!({
                "points": [thermal_report_test_point(60)],
                "settings": {}
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            legacy_dir.join("samples.ndjson"),
            format!(
                "{}\n",
                serde_json::to_string(&json!({
                    "targetTempC": 60,
                    "elapsedMs": 1000,
                    "status": {
                        "currentTempC": 60.2,
                        "heaterFilteredTempC": 60.1,
                        "heaterOutputPercent": 18,
                        "heaterPhysicalOutputPercent": 18,
                        "pdRequestMv": 21000
                    },
                    "phase": "hold",
                    "sourceTelemetry": {
                        "voltageMv": 21000,
                        "currentMa": 300,
                        "powerMw": 6300
                    }
                }))
                .unwrap()
            ),
        )
        .unwrap();

        let result =
            thermal_report::rerender_legacy_preliminary_review_bundle(
                thermal_report::ThermalLegacyReportInput {
                    legacy_bundle_dir: legacy_dir.clone(),
                    output_dir: Some(output_dir.clone()),
                },
            )
            .unwrap();
        let bundle: Value =
            serde_json::from_slice(&fs::read(output_dir.join("run.bundle.json")).unwrap()).unwrap();
        let html = fs::read_to_string(output_dir.join("index.html")).unwrap();

        assert_eq!(result["ok"], true);
        assert_eq!(
            result["operation"],
            "thermal_report.rerender_legacy_preliminary_review_bundle"
        );
        assert_eq!(bundle["kind"], "thermal_self_test_preliminary_bundle");
        assert_eq!(bundle["bundleDisposition"], "preliminary_review");
        assert_eq!(bundle["acceptedProfileRole"], "review_candidate_snapshot");
        assert_eq!(bundle["flagshipTargetsC"], json!([60]));
        assert_eq!(bundle["runs"][0]["target"], 60);
        assert_eq!(bundle["runs"][0]["pointSource"], "review_candidate_snapshot");
        assert_eq!(bundle["runs"][0]["rounds"][0]["attemptType"], "characterization");
        assert!(html.contains("60°C"));
        assert!(html.contains("preliminary review"));
    }

    #[test]
    fn thermal_report_rerender_live_bundle_splits_time_reset_attempts() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join("legacy-live");
        fs::create_dir_all(&legacy_dir).unwrap();

        let legacy_bundle = json!({
            "kind": "thermal_self_test_report_bundle",
            "runId": "legacy-live-run",
            "generatedAt": "2026-07-20T10:36:44.251Z",
            "selectedMode": "100w",
            "resolvedBank": "pps5a",
            "detectedSourceClass": "pps5a",
            "bundleDisposition": "latest_live_report",
            "acceptedProfileRole": "review_candidate_snapshot",
            "source": {
                "deviceId": "f293cc9c139e"
            },
            "target": {
                "deviceId": "serial-303a-1001-A0:F2:62:F2:0D:6C"
            },
            "parameters": {
                "holdSeconds": 60
            },
            "sourceRuns": {
                "60": "thermal-self-test-runs/source-run-summaries/60.run.json"
            },
            "candidateProfile": {
                "settings": {},
                "points": [thermal_report_test_point(60)]
            },
            "applied": [
                {
                    "targetTempC": 60,
                    "stopReason": "completed",
                    "maxOvershootC": 0.75,
                    "holdPeakToPeakC": 1.71,
                    "analysis": {
                        "holdMedianOutputPermille": 0,
                        "holdP90OutputPermille": 100,
                        "approachSource": {"powerMw": {"avg": 22425.0}},
                        "holdSource": {"powerMw": {"avg": 3935.0}}
                    },
                    "fullSpeedToStable": {
                        "limitMs": 10000,
                        "settleTimeMs": 8200,
                        "failureReason": Value::Null
                    },
                    "guard": {
                        "firstHoldAtMs": 9100
                    }
                }
            ],
            "validation": {
                "passed": true,
                "expectedTargetsC": [60],
                "failures": []
            }
        });
        fs::write(
            legacy_dir.join("run.bundle.json"),
            serde_json::to_vec_pretty(&legacy_bundle).unwrap(),
        )
        .unwrap();
        fs::write(
            legacy_dir.join("thermal-profile.accepted.json"),
            serde_json::to_vec_pretty(&json!({
                "points": [thermal_report_test_point(60)],
                "settings": {}
            }))
            .unwrap(),
        )
        .unwrap();
        let live_samples = vec![
            json!({
                "targetTempC": 60,
                "elapsedMs": 5000,
                "status": {
                    "currentTempC": 60.8,
                    "heaterFilteredTempC": 60.7,
                    "heaterOutputPercent": 16,
                    "heaterPhysicalOutputPercent": 16,
                    "pdRequestMv": 21000
                },
                "phase": "hold",
                "heaterParameters": thermal_report_test_point(60),
                "sourceTelemetry": {
                    "voltageMv": 21000,
                    "currentMa": 280,
                    "powerMw": 5880
                }
            }),
            json!({
                "targetTempC": 60,
                "elapsedMs": 1000,
                "status": {
                    "currentTempC": 60.2,
                    "heaterFilteredTempC": 60.1,
                    "heaterOutputPercent": 18,
                    "heaterPhysicalOutputPercent": 18,
                    "pdRequestMv": 21000
                },
                "phase": "hold",
                "heaterParameters": thermal_report_test_point(60),
                "sourceTelemetry": {
                    "voltageMv": 21000,
                    "currentMa": 300,
                    "powerMw": 6300
                }
            }),
        ];
        fs::write(
            legacy_dir.join("samples.ndjson"),
            live_samples
                .iter()
                .map(|sample| serde_json::to_string(sample).unwrap())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
        .unwrap();

        let result =
            thermal_report::rerender_legacy_preliminary_review_bundle(
                thermal_report::ThermalLegacyReportInput {
                    legacy_bundle_dir: legacy_dir.clone(),
                    output_dir: None,
                },
            )
            .unwrap();
        let output_dir = dir.path().join("legacy-live-rerendered");
        let bundle: Value =
            serde_json::from_slice(&fs::read(output_dir.join("run.bundle.json")).unwrap()).unwrap();
        let written_samples: Vec<Value> = fs::read_to_string(output_dir.join("samples.ndjson"))
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();

        assert_eq!(result["ok"], true);
        assert_eq!(bundle["kind"], "thermal_self_test_preliminary_bundle");
        assert_eq!(bundle["flagshipTargetsC"], json!([60]));
        assert_eq!(bundle["runs"][0]["roundCount"], 2);
        assert_eq!(bundle["runs"][0]["rounds"][0]["attemptType"], "legacy_live_report");
        assert_eq!(bundle["runs"][0]["rounds"][0]["selected"], false);
        assert_eq!(bundle["runs"][0]["rounds"][1]["selected"], true);
        assert_eq!(bundle["runs"][0]["holdCheck"]["confirmRunId"], "legacy-live-run-60");
        assert_eq!(bundle["runs"][0]["samples"][0]["t"], 1.0);
        assert_eq!(bundle["runs"][0]["rounds"][0]["samples"][0]["t"], 5.0);
        assert_eq!(bundle["runs"][0]["rounds"][1]["samples"][0]["t"], 1.0);
        assert_eq!(written_samples[0]["t"], 5.0);
        assert_eq!(written_samples[1]["t"], 1.0);
        assert!(
            result["outputDir"]
                .as_str()
                .unwrap()
                .ends_with("legacy-live-rerendered")
        );
    }

    #[derive(Clone)]
    struct RetuneApplyTestState {
        runtime_requests: Arc<Mutex<Vec<Value>>>,
        preview_enabled_readback: bool,
    }

    async fn create_retune_test_lease() -> Json<Value> {
        Json(json!({
            "leaseId": "lease-retune",
            "ttlMs": 60_000,
        }))
    }

    async fn heartbeat_retune_test_lease() -> Json<Value> {
        Json(json!({
            "leaseId": "lease-retune",
            "ttlMs": 60_000,
        }))
    }

    async fn release_retune_test_lease() -> Json<Value> {
        Json(json!({ "released": true }))
    }

    async fn capture_retune_preview(
        State(state): State<RetuneApplyTestState>,
        AxumPath(_device_id): AxumPath<String>,
        Json(payload): Json<Value>,
    ) -> Json<Value> {
        state.runtime_requests.lock().unwrap().push(payload.clone());
        Json(json!({
            "deviceId": "bench",
            "mode": "idle",
            "targetTempC": 60,
            "currentTempC": 25.0,
            "heaterEnabled": false,
            "thermalControlProfilePreview": true,
        }))
    }

    async fn retune_status_readback(
        State(state): State<RetuneApplyTestState>,
        AxumPath(_device_id): AxumPath<String>,
    ) -> Json<Value> {
        Json(json!({
            "deviceId": "bench",
            "mode": "idle",
            "targetTempC": 60,
            "currentTempC": 25.0,
            "heaterEnabled": false,
            "thermalControlProfilePreview": state.preview_enabled_readback,
            "thermalControl": {
                "profileActive": state.preview_enabled_readback,
                "source": if state.preview_enabled_readback { "preview" } else { "default" },
            },
        }))
    }

    async fn failing_retune_preview(
        State(state): State<RetuneApplyTestState>,
        AxumPath(_device_id): AxumPath<String>,
        Json(payload): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        state.runtime_requests.lock().unwrap().push(payload);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "preview failed"})),
        )
    }

    async fn spawn_retune_apply_server(
        preview_enabled_readback: bool,
        fail_preview: bool,
    ) -> (String, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
        let runtime_requests = Arc::new(Mutex::new(Vec::new()));
        let state = RetuneApplyTestState {
            runtime_requests: runtime_requests.clone(),
            preview_enabled_readback,
        };
        let runtime_route = if fail_preview {
            put(failing_retune_preview)
        } else {
            put(capture_retune_preview)
        };
        let app = Router::new()
            .route(
                "/api/v1/devices/{device_id}/leases",
                post(create_retune_test_lease),
            )
            .route(
                "/api/v1/leases/{lease_id}/heartbeat",
                post(heartbeat_retune_test_lease),
            )
            .route(
                "/api/v1/leases/{lease_id}",
                delete(release_retune_test_lease),
            )
            .route("/api/v1/devices/{device_id}/runtime", runtime_route)
            .route(
                "/api/v1/devices/{device_id}/status",
                get(retune_status_readback),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}"), runtime_requests, server)
    }

    #[derive(Clone)]
    struct ThermalStatusRetryTestState {
        attempts: Arc<AtomicUsize>,
        delayed_attempts: usize,
        delay_ms: u64,
    }

    async fn flaky_thermal_status_readback(
        State(state): State<ThermalStatusRetryTestState>,
        AxumPath(_device_id): AxumPath<String>,
    ) -> Json<Value> {
        let attempt = state.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt <= state.delayed_attempts {
            tokio::time::sleep(Duration::from_millis(state.delay_ms)).await;
        }
        Json(json!({
            "attempt": attempt,
            "currentTempC": 25.0,
            "heaterEnabled": false,
            "thermalControlProfilePreview": false,
        }))
    }

    async fn spawn_flaky_thermal_status_server(
        delayed_attempts: usize,
        delay_ms: u64,
    ) -> (
        ResolvedUsbTarget,
        Arc<AtomicUsize>,
        tokio::task::JoinHandle<()>,
    ) {
        let attempts = Arc::new(AtomicUsize::new(0));
        let state = ThermalStatusRetryTestState {
            attempts: attempts.clone(),
            delayed_attempts,
            delay_ms,
        };
        let app = Router::new()
            .route(
                "/api/v1/devices/{device_id}/status",
                get(flaky_thermal_status_readback),
            )
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (
            ResolvedUsbTarget {
                device: "bench".to_string(),
                devd: format!("http://{addr}"),
                hardware_id: None,
            },
            attempts,
            server,
        )
    }

    #[tokio::test]
    async fn thermal_status_retry_recovers_after_single_timeout() {
        let (resolved, attempts, server) = spawn_flaky_thermal_status_server(1, 150).await;

        let status = request_thermal_status_with_retry_config(
            &Client::new(),
            &resolved,
            "lease-test",
            Duration::from_millis(50),
            2,
        )
        .await
        .unwrap();

        assert_eq!(status["attempt"], 2);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);

        server.abort();
    }

    #[tokio::test]
    async fn thermal_retune_apply_preview_writes_verified_receipt_and_uses_candidate() {
        let dir = tempfile::tempdir().unwrap();
        write_retune_fixture(dir.path());
        let (base_url, runtime_requests, server) = spawn_retune_apply_server(true, false).await;

        let summary = retune_thermal_self_test_run(
            &Client::new(),
            &base_url,
            ThermalRetuneArgs {
                target: TargetSelector {
                    device: Some("bench".to_string()),
                    hardware: None,
                },
                run_dir: dir.path().to_path_buf(),
                optimize_targets_c: None,
                apply_preview: true,
            },
        )
        .await
        .unwrap();

        let replay_summary: Value =
            serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap())
                .unwrap();
        assert_eq!(summary["applyPreview"]["ok"], true);
        assert_eq!(replay_summary["applyPreview"]["ok"], true);
        assert_eq!(
            replay_summary["applyPreview"]["statusReadback"]["thermalControlProfilePreview"],
            true
        );
        let requests = runtime_requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["leaseId"], "lease-retune");
        assert_eq!(
            requests[0]["thermalControlProfile"]["op"].as_str(),
            Some("preview")
        );
        assert_eq!(
            requests[0]["thermalControlProfile"]["profile"],
            replay_summary["candidateProfile"]
        );

        server.abort();
    }

    #[tokio::test]
    async fn thermal_retune_apply_preview_failure_preserves_replay_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        write_retune_fixture(dir.path());
        let (base_url, _runtime_requests, server) = spawn_retune_apply_server(true, true).await;

        let error = retune_thermal_self_test_run(
            &Client::new(),
            &base_url,
            ThermalRetuneArgs {
                target: TargetSelector {
                    device: Some("bench".to_string()),
                    hardware: None,
                },
                run_dir: dir.path().to_path_buf(),
                optimize_targets_c: None,
                apply_preview: true,
            },
        )
        .await
        .unwrap_err()
        .to_string();

        let replay_summary: Value =
            serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap())
                .unwrap();
        assert!(error.contains("HTTP 500"));
        assert_eq!(replay_summary["applyPreview"]["ok"], false);
        assert!(
            replay_summary["applyPreview"]["error"]
                .as_str()
                .unwrap()
                .contains("HTTP 500")
        );
        assert!(
            dir.path()
                .join("thermal-profile.replayed.candidate.json")
                .exists()
        );

        server.abort();
    }

    #[tokio::test]
    async fn thermal_retune_apply_preview_target_error_preserves_replay_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        write_retune_fixture(dir.path());

        let error = retune_thermal_self_test_run(
            &Client::new(),
            DEFAULT_DEVD_URL,
            ThermalRetuneArgs {
                target: TargetSelector {
                    device: None,
                    hardware: None,
                },
                run_dir: dir.path().to_path_buf(),
                optimize_targets_c: None,
                apply_preview: true,
            },
        )
        .await
        .unwrap_err()
        .to_string();

        let replay_summary: Value =
            serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap())
                .unwrap();
        assert!(error.contains("requires --device or --hardware"));
        assert_eq!(replay_summary["applyPreview"]["ok"], false);
        assert_eq!(
            replay_summary["applyPreview"]["target"]["devd"],
            DEFAULT_DEVD_URL
        );
        assert!(
            dir.path()
                .join("thermal-profile.replayed.candidate.json")
                .exists()
        );
    }

    #[tokio::test]
    async fn thermal_retune_apply_preview_rejects_ambiguous_target_and_preserves_replay_artifacts()
    {
        let dir = tempfile::tempdir().unwrap();
        write_retune_fixture(dir.path());

        let error = retune_thermal_self_test_run(
            &Client::new(),
            DEFAULT_DEVD_URL,
            ThermalRetuneArgs {
                target: TargetSelector {
                    device: Some("bench".to_string()),
                    hardware: Some("saved-bench".to_string()),
                },
                run_dir: dir.path().to_path_buf(),
                optimize_targets_c: None,
                apply_preview: true,
            },
        )
        .await
        .unwrap_err()
        .to_string();

        let replay_summary: Value =
            serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap())
                .unwrap();
        assert!(error.contains("accepts only one of --device or --hardware"));
        assert_eq!(replay_summary["applyPreview"]["ok"], false);
        assert_eq!(
            replay_summary["applyPreview"]["target"]["deviceId"],
            "bench"
        );
        assert_eq!(
            replay_summary["applyPreview"]["target"]["hardwareId"],
            "saved-bench"
        );
        assert!(
            dir.path()
                .join("thermal-profile.replayed.candidate.json")
                .exists()
        );
    }

    #[tokio::test]
    async fn thermal_retune_apply_preview_missing_saved_hardware_preserves_replay_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        write_retune_fixture(dir.path());

        let missing_hardware_id = "missing-retune-hardware-7c6c7596f0b64e75";
        let error = retune_thermal_self_test_run(
            &Client::new(),
            DEFAULT_DEVD_URL,
            ThermalRetuneArgs {
                target: TargetSelector {
                    device: None,
                    hardware: Some(missing_hardware_id.to_string()),
                },
                run_dir: dir.path().to_path_buf(),
                optimize_targets_c: None,
                apply_preview: true,
            },
        )
        .await
        .unwrap_err()
        .to_string();

        let replay_summary: Value =
            serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap())
                .unwrap();
        assert!(error.contains("saved hardware not found"));
        assert_eq!(replay_summary["applyPreview"]["ok"], false);
        assert_eq!(
            replay_summary["applyPreview"]["target"]["hardwareId"],
            missing_hardware_id
        );
        assert!(
            dir.path()
                .join("thermal-profile.replayed.candidate.json")
                .exists()
        );
    }

    #[tokio::test]
    async fn thermal_retune_apply_preview_requires_status_readback_preview_flag() {
        let dir = tempfile::tempdir().unwrap();
        write_retune_fixture(dir.path());
        let (base_url, _runtime_requests, server) = spawn_retune_apply_server(false, false).await;

        let error = retune_thermal_self_test_run(
            &Client::new(),
            &base_url,
            ThermalRetuneArgs {
                target: TargetSelector {
                    device: Some("bench".to_string()),
                    hardware: None,
                },
                run_dir: dir.path().to_path_buf(),
                optimize_targets_c: None,
                apply_preview: true,
            },
        )
        .await
        .unwrap_err()
        .to_string();

        let replay_summary: Value =
            serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap())
                .unwrap();
        assert!(error.contains("thermalControlProfilePreview"));
        assert_eq!(replay_summary["applyPreview"]["ok"], false);
        assert_eq!(
            replay_summary["applyPreview"]["statusReadback"]["thermalControlProfilePreview"],
            false
        );

        server.abort();
    }
    #[tokio::test]
    async fn flash_with_lease_reuses_same_lease_for_dry_run_and_real_flash() {
        #[derive(Clone)]
        struct FlashTestState {
            requests: Arc<Mutex<Vec<Value>>>,
        }

        async fn create_test_lease() -> Json<Value> {
            Json(json!({
                "leaseId": "lease-test",
                "ttlMs": 60_000,
            }))
        }

        async fn heartbeat_test_lease() -> Json<Value> {
            Json(json!({
                "leaseId": "lease-test",
                "ttlMs": 60_000,
            }))
        }

        async fn release_test_lease() -> Json<Value> {
            Json(json!({ "released": true }))
        }

        async fn capture_flash(
            State(state): State<FlashTestState>,
            AxumPath(_device_id): AxumPath<String>,
            Json(payload): Json<Value>,
        ) -> Json<Value> {
            state.requests.lock().unwrap().push(payload.clone());
            let dry_run = payload
                .get("dryRun")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Json(json!({
                "artifactId": payload["artifact"]["artifactId"],
                "dryRun": dry_run,
                "status": if dry_run { "passed" } else { "flashed" },
                "message": "ok",
            }))
        }

        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = FlashTestState {
            requests: requests.clone(),
        };
        let app = Router::new()
            .route(
                "/api/v1/devices/{device_id}/leases",
                post(create_test_lease),
            )
            .route(
                "/api/v1/leases/{lease_id}/heartbeat",
                post(heartbeat_test_lease),
            )
            .route("/api/v1/leases/{lease_id}", delete(release_test_lease))
            .route("/api/v1/devices/{device_id}/flash", post(capture_flash))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let artifact = FirmwareArtifact {
            artifact_id: "a".to_string(),
            name: "A".to_string(),
            version: "v".to_string(),
            git_sha: "sha".to_string(),
            build_id: "build".to_string(),
            target_chip: "esp32s3".to_string(),
            profile: "release".to_string(),
            features: vec!["web_serial".to_string()],
            protocol: "flux-purr.usb.v1".to_string(),
            files: Vec::new(),
        };

        let result = flash_with_lease(
            &Client::new(),
            ResolvedUsbTarget {
                device: "bench".to_string(),
                devd: format!("http://{addr}"),
                hardware_id: None,
            },
            artifact,
            false,
            Some("FLASH".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(result["status"], "flashed");
        let captured = requests.lock().unwrap().clone();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0]["leaseId"], "lease-test");
        assert_eq!(captured[1]["leaseId"], "lease-test");
        assert_eq!(captured[0]["dryRun"], true);
        assert_eq!(captured[1]["dryRun"], false);
        assert_eq!(captured[1]["confirm"], "FLASH");

        server.abort();
    }
}
