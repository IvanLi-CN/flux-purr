use std::{
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use clap::{ArgAction, Args, Parser, Subcommand};
use flux_purr_devd::{
    DEFAULT_DEVD_URL, FirmwareArtifact, FirmwareArtifactCatalog, WifiConfigOp,
    hardware_registry_path, read_user_config, write_user_config,
};
use reqwest::{Client, Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
}

#[derive(Debug, Subcommand)]
enum ThermalProfileCommand {
    Preview(ThermalProfileFileArgs),
    ClearPreview(TargetSelector),
    Save(ThermalProfileFileArgs),
    ClearSaved(TargetSelector),
}

#[derive(Debug, Args)]
struct ThermalProfileFileArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(long)]
    file: PathBuf,
}

#[derive(Debug, Args)]
struct ThermalSelfTestArgs {
    #[command(flatten)]
    target: TargetSelector,
    #[arg(
        long = "source-device-id",
        help = "Released IsolaPurr device id used as the external bench source."
    )]
    source_device_id: String,
    #[arg(long = "source-voltage-v", default_value = "20.0")]
    source_voltage_v: String,
    #[arg(long = "source-current-a", default_value = "3.25")]
    source_current_a: String,
    #[arg(long = "sample-interval-ms", default_value_t = 500)]
    sample_interval_ms: u64,
    #[arg(long = "hold-seconds", default_value_t = 60)]
    hold_seconds: u64,
    #[arg(long = "stage-timeout-seconds", default_value_t = 300)]
    stage_timeout_seconds: u64,
    #[arg(long = "runtime-rearm-attempts", default_value_t = 3)]
    runtime_rearm_attempts: u8,
    #[arg(long = "cooldown-temp-c", default_value_t = 40.0)]
    cooldown_temp_c: f64,
    #[arg(long = "cooldown-timeout-seconds", default_value_t = 7200)]
    cooldown_timeout_seconds: u64,
    #[arg(long = "output-dir", default_value = "thermal-self-test-runs")]
    output_dir: PathBuf,
    #[arg(long = "dry-run", action = ArgAction::SetTrue)]
    dry_run: bool,
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
    Ok(request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?)
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
    Ok(request
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?)
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

const THERMAL_SELF_TEST_TARGETS_C: [i16; 9] = [50, 100, 120, 150, 180, 200, 210, 220, 250];

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
                    Some(json!({
                        "thermalControlProfile": {
                            "op": "preview",
                            "profile": profile
                        }
                    })),
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
                request_with_lease(
                    client,
                    resolve_target(args.target, default_devd)?,
                    Method::PUT,
                    "/runtime",
                    Some(json!({
                        "thermalControlProfile": {
                            "op": "save",
                            "profile": profile
                        }
                    })),
                )
                .await
            }
            ThermalProfileCommand::ClearSaved(selector) => {
                request_with_lease(
                    client,
                    resolve_target(selector, default_devd)?,
                    Method::PUT,
                    "/runtime",
                    Some(json!({
                        "thermalControlProfile": {
                            "op": "clear_saved"
                        }
                    })),
                )
                .await
            }
        },
        ThermalCommand::SelfTest(args) => {
            collect_thermal_self_test(client, default_devd, args).await
        }
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
    started_at: Option<tokio::time::Instant>,
    rise_time_ms: Option<u64>,
    min_c: f64,
    max_c: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThermalRuntimeDropReason {
    UptimeReset,
    HeaterDisarmed,
    WrongMode,
    WrongTarget,
}

impl ThermalRuntimeDropReason {
    fn as_str(self) -> &'static str {
        match self {
            ThermalRuntimeDropReason::UptimeReset => "uptime_reset",
            ThermalRuntimeDropReason::HeaterDisarmed => "heater_disarmed",
            ThermalRuntimeDropReason::WrongMode => "wrong_mode",
            ThermalRuntimeDropReason::WrongTarget => "wrong_target",
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
            started_at: None,
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
    ) -> ThermalHoldObservation {
        let target_temp_c = f64::from(self.target_temp_c);
        if current_temp_c < target_temp_c - self.entered_threshold_c {
            self.started_at = None;
            self.min_c = f64::INFINITY;
            self.max_c = f64::NEG_INFINITY;
            return ThermalHoldObservation::Warmup;
        }
        if (current_temp_c - target_temp_c).abs() > self.stable_band_c {
            self.started_at = None;
            self.min_c = f64::INFINITY;
            self.max_c = f64::NEG_INFINITY;
            return ThermalHoldObservation::Warmup;
        }

        let started_at = *self.started_at.get_or_insert_with(|| {
            self.rise_time_ms.get_or_insert(elapsed_ms);
            now
        });
        self.min_c = self.min_c.min(current_temp_c);
        self.max_c = self.max_c.max(current_temp_c);
        if now.duration_since(started_at) >= self.hold_duration {
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

impl ThermalStageResult {
    fn to_value(&self) -> Value {
        json!({
            "targetTempC": self.target_temp_c,
            "riseTimeMs": self.rise_time_ms,
            "maxOvershootC": self.max_overshoot_c,
            "holdPeakToPeakC": self.hold_peak_to_peak_c,
            "sampleCount": self.sample_count,
            "stopReason": self.stop_reason,
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

async fn collect_thermal_self_test(
    client: &Client,
    default_devd: &str,
    args: ThermalSelfTestArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let resolved = resolve_target(args.target.clone(), default_devd)?;
    let source_voltage_mv = parse_pps_volts(&args.source_voltage_v)?;
    let source_current_ma = parse_pps_amps(&args.source_current_a)?;
    let candidate_profile = default_thermal_candidate_profile();
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
        serde_json::to_vec_pretty(&candidate_profile)?,
    )?;
    let mut samples_writer = BufWriter::new(File::create(&samples_path)?);

    let mut sample_index = 0usize;
    let mut baseline_results = Vec::new();
    let mut preview_results = Vec::new();
    let mut run_error = None::<String>;

    if args.dry_run {
        baseline_results = write_dry_thermal_ladder(
            &mut samples_writer,
            &run_id,
            "baseline",
            source_voltage_mv,
            source_current_ma,
            &mut sample_index,
        )?;
        preview_results = write_dry_thermal_ladder(
            &mut samples_writer,
            &run_id,
            "preview",
            source_voltage_mv,
            source_current_ma,
            &mut sample_index,
        )?;
    } else {
        validate_isolapurr_tools()?;
        let lease = create_lease(client, &resolved).await?;
        let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
        let source_result = set_isolapurr_bench_output(
            &args.source_device_id,
            source_voltage_mv,
            source_current_ma,
        );

        let test_result = async {
            source_result?;
            request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "heaterEnabled": false,
                    "thermalControlProfile": {
                        "op": "clear_preview"
                    }
                })),
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
            baseline_results = run_thermal_ladder(
                client,
                &resolved,
                &lease.lease_id,
                &mut samples_writer,
                &run_id,
                "baseline",
                source_voltage_mv,
                source_current_ma,
                &args,
                &mut sample_index,
            )
            .await?;

            request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::PUT,
                "/runtime",
                Some(json!({"heaterEnabled": false})),
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
            request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "thermalControlProfile": {
                        "op": "preview",
                        "profile": candidate_profile.clone(),
                    }
                })),
            )
            .await?;
            preview_results = run_thermal_ladder(
                client,
                &resolved,
                &lease.lease_id,
                &mut samples_writer,
                &run_id,
                "preview",
                source_voltage_mv,
                source_current_ma,
                &args,
                &mut sample_index,
            )
            .await?;
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        }
        .await;

        if let Err(error) = test_result {
            run_error = Some(error.to_string());
        }
        let _ = request_leased(
            client,
            &resolved,
            &lease.lease_id,
            Method::PUT,
            "/runtime",
            Some(json!({
                "heaterEnabled": false,
                "thermalControlProfile": {
                    "op": "clear_preview"
                }
            })),
        )
        .await;
        let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
        heartbeat.abort();
        if let Err(error) = set_isolapurr_output_auto(&args.source_device_id)
            && run_error.is_none()
        {
            run_error = Some(format!("isolapurr cleanup failed: {error}"));
        }
    }

    samples_writer.flush()?;
    let validation = validate_thermal_preview_results(&baseline_results, &preview_results);
    let complete = run_error.is_none() && validation["passed"].as_bool() == Some(true);
    let summary = json!({
        "kind": "thermal_self_test",
        "ok": complete,
        "runId": run_id,
        "dryRun": args.dry_run,
        "target": {
            "deviceId": resolved.device,
            "hardwareId": resolved.hardware_id,
            "devd": resolved.devd,
        },
        "source": {
            "deviceId": args.source_device_id,
            "mode": "isolapurr_manual_pps_bench",
            "voltageMv": source_voltage_mv,
            "currentLimitMa": source_current_ma,
            "usbCPath": "forced-on",
        },
        "parameters": {
            "targetsC": THERMAL_SELF_TEST_TARGETS_C,
            "sampleIntervalMs": args.sample_interval_ms.max(1),
            "holdSeconds": args.hold_seconds.max(1),
            "stageTimeoutSeconds": args.stage_timeout_seconds.max(1),
            "cooldownTempC": args.cooldown_temp_c,
            "cooldownTimeoutSeconds": args.cooldown_timeout_seconds.max(1),
            "limits": {
                "overshootC": 3.0,
                "holdPeakToPeakC": 3.0,
                "riseTimeRegressionRatio": 1.15
            }
        },
        "files": {
            "runDir": run_dir,
            "summaryPath": summary_path,
            "samplesPath": samples_path,
            "candidateProfilePath": candidate_path,
        },
        "candidateProfile": candidate_profile.clone(),
        "baseline": baseline_results.iter().map(ThermalStageResult::to_value).collect::<Vec<_>>(),
        "preview": preview_results.iter().map(ThermalStageResult::to_value).collect::<Vec<_>>(),
        "validation": validation,
        "sampleCount": sample_index,
        "complete": complete,
        "error": run_error,
    });
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

fn default_thermal_candidate_profile() -> Value {
    let mut points = THERMAL_SELF_TEST_TARGETS_C
        .iter()
        .copied()
        .map(|target_temp_c| {
            let (brake_distance_centi_c, approach_power_permille, hold_power_permille) =
                if target_temp_c <= 100 {
                    (450, 380, 180)
                } else if target_temp_c <= 180 {
                    (700, 320, 220)
                } else {
                    (1_000, 260, 260)
                };
            json!({
                "targetTempC": target_temp_c,
                "brakeDistanceCentiC": brake_distance_centi_c,
                "approachPowerPermille": approach_power_permille,
                "holdPowerPermille": hold_power_permille,
            })
        })
        .collect::<Vec<_>>();
    points.push(Value::Null);
    json!({
        "settings": {
            "tempFilterAlphaPermille": 450,
            "warmupReenterCentiC": 400,
            "holdEntryCentiC": 90,
            "holdExitCentiC": 200,
            "holdOnCentiC": 30,
            "holdOffCentiC": 5,
            "overshootCutoffCentiC": 25,
            "approachMaxTicks": 5,
            "holdKpPermillePerC": 120,
            "holdKiPermillePerCTick": 12
        },
        "points": points
    })
}

fn write_dry_thermal_ladder(
    samples_writer: &mut BufWriter<File>,
    run_id: &str,
    test_phase: &'static str,
    source_voltage_mv: u16,
    source_current_ma: u16,
    sample_index: &mut usize,
) -> Result<Vec<ThermalStageResult>, Box<dyn std::error::Error + Send + Sync>> {
    let mut results = Vec::new();
    for target_temp_c in THERMAL_SELF_TEST_TARGETS_C {
        let baseline = test_phase == "baseline";
        let rise_time_ms = if baseline {
            u64::from(target_temp_c as u16) * 1_000
        } else {
            u64::from(target_temp_c as u16) * 980
        };
        let max_overshoot_c = if baseline { 2.6 } else { 1.8 };
        let hold_peak_to_peak_c = if baseline { 2.4 } else { 1.6 };
        for phase in ["warmup", "hold"] {
            let temp_c = if phase == "warmup" {
                f64::from(target_temp_c) - 0.5
            } else {
                f64::from(target_temp_c) + max_overshoot_c / 2.0
            };
            let sample = json!({
                "runId": run_id,
                "sampleIndex": *sample_index,
                "capturedAtUnixMs": current_unix_millis(),
                "testPhase": test_phase,
                "phase": phase,
                "targetTempC": target_temp_c,
                "source": {
                    "voltageMv": source_voltage_mv,
                    "currentLimitMa": source_current_ma,
                },
                "status": synthetic_thermal_status(target_temp_c, temp_c, source_voltage_mv, source_current_ma),
            });
            writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
            *sample_index = sample_index.saturating_add(1);
        }
        results.push(ThermalStageResult {
            target_temp_c,
            rise_time_ms,
            max_overshoot_c,
            hold_peak_to_peak_c,
            sample_count: 2,
            stop_reason: "completed",
        });
    }
    Ok(results)
}

fn synthetic_thermal_status(
    target_temp_c: i16,
    current_temp_c: f64,
    source_voltage_mv: u16,
    source_current_ma: u16,
) -> Value {
    json!({
        "mode": "sampling",
        "heaterEnabled": true,
        "heaterOutputPercent": 26,
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

async fn run_thermal_ladder(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    samples_writer: &mut BufWriter<File>,
    run_id: &str,
    test_phase: &'static str,
    source_voltage_mv: u16,
    source_current_ma: u16,
    args: &ThermalSelfTestArgs,
    sample_index: &mut usize,
) -> Result<Vec<ThermalStageResult>, Box<dyn std::error::Error + Send + Sync>> {
    let mut results = Vec::new();
    for target_temp_c in THERMAL_SELF_TEST_TARGETS_C {
        arm_thermal_self_test_target(client, resolved, lease_id, target_temp_c).await?;
        let result = run_thermal_stage(
            client,
            resolved,
            lease_id,
            samples_writer,
            run_id,
            test_phase,
            target_temp_c,
            source_voltage_mv,
            source_current_ma,
            args,
            sample_index,
        )
        .await?;
        if result.stop_reason != "completed" {
            arm_thermal_self_test_heater(client, resolved, lease_id, false, target_temp_c).await?;
            results.push(result);
            break;
        }
        results.push(result);
    }
    Ok(results)
}

async fn arm_thermal_self_test_target(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    target_temp_c: i16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    arm_thermal_self_test_heater(client, resolved, lease_id, true, target_temp_c).await
}

async fn arm_thermal_self_test_heater(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    heater_enabled: bool,
    target_temp_c: i16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    request_leased(
        client,
        resolved,
        lease_id,
        Method::PUT,
        "/runtime",
        Some(json!({
            "heaterEnabled": heater_enabled,
            "targetTempC": target_temp_c,
        })),
    )
    .await?;
    Ok(())
}

async fn run_thermal_stage(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    samples_writer: &mut BufWriter<File>,
    run_id: &str,
    test_phase: &'static str,
    target_temp_c: i16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    args: &ThermalSelfTestArgs,
    sample_index: &mut usize,
) -> Result<ThermalStageResult, Box<dyn std::error::Error + Send + Sync>> {
    let started = tokio::time::Instant::now();
    let deadline = started + Duration::from_secs(args.stage_timeout_seconds.max(1));
    let hold_duration = Duration::from_secs(args.hold_seconds.max(1));
    let sample_interval = Duration::from_millis(args.sample_interval_ms.max(1));
    let mut next_tick = started;
    let mut hold_tracker = ThermalHoldTracker::new(target_temp_c, hold_duration);
    let mut max_temp_c = f64::NEG_INFINITY;
    let mut stage_sample_count = 0usize;
    let mut stop_reason = "timeout";
    let mut last_uptime_seconds = None::<u64>;
    let mut runtime_rearm_attempts = args.runtime_rearm_attempts;

    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        let status =
            request_leased(client, resolved, lease_id, Method::GET, "/status", None).await?;
        let runtime_drop_reason =
            thermal_runtime_drop_reason(&status, target_temp_c, last_uptime_seconds);
        if let Some(reason) = runtime_drop_reason {
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
                    "voltageMv": source_voltage_mv,
                    "currentLimitMa": source_current_ma,
                },
                "runtimeDropReason": reason.as_str(),
                "runtimeRearmAttemptsRemaining": runtime_rearm_attempts,
                "status": status,
            });
            writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
            samples_writer.flush()?;
            *sample_index = sample_index.saturating_add(1);
            stage_sample_count = stage_sample_count.saturating_add(1);
            if runtime_rearm_attempts == 0 {
                stop_reason = "runtime_lost";
                break;
            }
            runtime_rearm_attempts = runtime_rearm_attempts.saturating_sub(1);
            arm_thermal_self_test_target(client, resolved, lease_id, target_temp_c).await?;
            hold_tracker = ThermalHoldTracker::new(target_temp_c, hold_duration);
            max_temp_c = f64::NEG_INFINITY;
            last_uptime_seconds = None;
            next_tick = tokio::time::Instant::now() + sample_interval;
            tokio::time::sleep(sample_interval).await;
            continue;
        }
        last_uptime_seconds = status.get("uptimeSeconds").and_then(Value::as_u64);
        let current_temp_c = require_status_f64(&status, "currentTempC")?;
        max_temp_c = max_temp_c.max(current_temp_c);
        let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let observation = hold_tracker.observe(current_temp_c, elapsed_ms, now);
        let phase = match observation {
            ThermalHoldObservation::Warmup => "warmup",
            ThermalHoldObservation::Hold | ThermalHoldObservation::Completed => {
                if observation == ThermalHoldObservation::Completed {
                    stop_reason = "completed";
                }
                "hold"
            }
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
                "voltageMv": source_voltage_mv,
                "currentLimitMa": source_current_ma,
            },
            "status": status,
        });
        writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
        samples_writer.flush()?;
        *sample_index = sample_index.saturating_add(1);
        stage_sample_count = stage_sample_count.saturating_add(1);
        if stop_reason == "completed" {
            break;
        }
        next_tick += sample_interval;
        tokio::time::sleep_until(next_tick).await;
    }
    if stop_reason != "completed" {
        arm_thermal_self_test_heater(client, resolved, lease_id, false, target_temp_c).await?;
    }

    Ok(ThermalStageResult {
        target_temp_c,
        rise_time_ms: hold_tracker
            .rise_time_ms()
            .unwrap_or_else(|| started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
        max_overshoot_c: (max_temp_c - f64::from(target_temp_c)).max(0.0),
        hold_peak_to_peak_c: hold_tracker.peak_to_peak_c(),
        sample_count: stage_sample_count,
        stop_reason,
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
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

fn validate_thermal_preview_results(
    baseline: &[ThermalStageResult],
    preview: &[ThermalStageResult],
) -> Value {
    let mut failures = Vec::new();
    for preview_stage in preview {
        let Some(baseline_stage) = baseline
            .iter()
            .find(|stage| stage.target_temp_c == preview_stage.target_temp_c)
        else {
            failures.push(json!({
                "targetTempC": preview_stage.target_temp_c,
                "reason": "missing_baseline",
            }));
            continue;
        };
        if baseline_stage.stop_reason != "completed" {
            failures.push(json!({
                "targetTempC": baseline_stage.target_temp_c,
                "phase": "baseline",
                "reason": "incomplete_stage",
                "stopReason": baseline_stage.stop_reason,
            }));
        }
        if preview_stage.stop_reason != "completed" {
            failures.push(json!({
                "targetTempC": preview_stage.target_temp_c,
                "phase": "preview",
                "reason": "incomplete_stage",
                "stopReason": preview_stage.stop_reason,
            }));
        }
        if preview_stage.max_overshoot_c > 3.0 {
            failures.push(json!({
                "targetTempC": preview_stage.target_temp_c,
                "reason": "overshoot",
                "value": preview_stage.max_overshoot_c,
                "limit": 3.0,
            }));
        }
        if preview_stage.hold_peak_to_peak_c > 3.0 {
            failures.push(json!({
                "targetTempC": preview_stage.target_temp_c,
                "reason": "hold_p2p",
                "value": preview_stage.hold_peak_to_peak_c,
                "limit": 3.0,
            }));
        }
        let rise_limit_ms = (baseline_stage.rise_time_ms as f64 * 1.15).ceil() as u64;
        if preview_stage.rise_time_ms > rise_limit_ms {
            failures.push(json!({
                "targetTempC": preview_stage.target_temp_c,
                "reason": "rise_time_regression",
                "valueMs": preview_stage.rise_time_ms,
                "limitMs": rise_limit_ms,
            }));
        }
    }
    json!({
        "passed": failures.is_empty() && !baseline.is_empty() && !preview.is_empty(),
        "failures": failures,
    })
}

fn validate_isolapurr_tools() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for tool in ["isolapurr", "isolapurr-devd"] {
        let status = ProcessCommand::new(tool).arg("--help").status().map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("{tool} --help failed to start; install released IsolaPurr host tools: {error}"),
            )
        })?;
        if !status.success() {
            return Err(format!("{tool} --help exited with {status}").into());
        }
    }
    Ok(())
}

fn set_isolapurr_bench_output(
    device_id: &str,
    voltage_mv: u16,
    current_limit_ma: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(device_id)?;
    let output = ProcessCommand::new("isolapurr")
        .args([
            "power",
            "output",
            "manual",
            "--device-id",
            device_id,
            "--voltage-mv",
            &voltage_mv.to_string(),
            "--current-limit-ma",
            &current_limit_ma.to_string(),
            "--usb-c-path",
            "forced-on",
        ])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "isolapurr power output manual exited with {}; stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }

    let config = read_isolapurr_power_config(device_id)?;
    if !isolapurr_power_config_value_matches_manual(&config, voltage_mv, current_limit_ma) {
        return Err(format!(
            "isolapurr power output manual readback mismatch for device_id={device_id}"
        )
        .into());
    }
    Ok(())
}

fn set_isolapurr_output_auto(
    device_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(device_id)?;
    let output = ProcessCommand::new("isolapurr")
        .args(["power", "output", "auto", "--device-id", device_id])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "isolapurr power output auto exited with {}; stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }

    let config = read_isolapurr_power_config(device_id)?;
    if !isolapurr_power_config_value_is_auto(&config) {
        return Err(format!(
            "isolapurr power output auto readback mismatch for device_id={device_id}"
        )
        .into());
    }
    Ok(())
}

fn validate_isolapurr_device_identity(
    device_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let output = ProcessCommand::new("isolapurr")
        .args(["status", "--device-id", device_id, "--json"])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "isolapurr status exited with {}; stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let status: Value = serde_json::from_slice(&output.stdout)?;
    if !isolapurr_status_identity_matches(&status, device_id) {
        let actual = isolapurr_status_device_id(&status).unwrap_or("unknown");
        return Err(format!(
            "isolapurr identity mismatch requested_device_id={device_id} actual_device_id={actual}"
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
        .get("manual")
        .and_then(Value::as_object)
        .is_some_and(|manual| {
            json_u64_any(manual, &["voltage_mv", "voltageMv"]) == Some(u64::from(voltage_mv))
                && json_u64_any(manual, &["current_limit_ma", "currentLimitMa"])
                    == Some(u64::from(current_limit_ma))
                && json_str_any(manual, &["usb_c_path_mode", "usbCPathMode"]) == Some("force")
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

fn read_isolapurr_power_config(
    device_id: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let output = ProcessCommand::new("isolapurr")
        .args([
            "power",
            "config",
            "show",
            "--device-id",
            device_id,
            "--json",
        ])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "isolapurr power config show exited with {}; stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
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
    Ok(json!({
        "mode": status
            .get("mode")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: mode"))?,
        "heaterEnabled": status
            .get("heaterEnabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: heaterEnabled"))?,
        "heaterOutputPercent": require_status_u64(status, "heaterOutputPercent")?,
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
                Some(json!({"heaterEnabled": false})),
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
            Some(json!({"heaterEnabled": false})),
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
    Ok(client
        .post(api_url(&resolved.devd, &path)?)
        .send()
        .await?
        .error_for_status()?
        .json::<Lease>()
        .await?)
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
    use std::sync::{Arc, Mutex};

    use axum::{
        Json, Router,
        extract::{Path as AxumPath, State},
        routing::{delete, post},
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
    fn thermal_candidate_profile_excludes_300c() {
        let profile = default_thermal_candidate_profile();
        let targets = profile["points"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|point| point.get("targetTempC").and_then(Value::as_i64))
            .collect::<Vec<_>>();
        assert_eq!(targets, vec![50, 100, 120, 150, 180, 200, 210, 220, 250]);
        assert_eq!(profile["points"].as_array().unwrap().len(), 10);
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
    fn thermal_validation_rejects_incomplete_stages() {
        let baseline = vec![ThermalStageResult {
            target_temp_c: 120,
            rise_time_ms: 120_000,
            max_overshoot_c: 1.0,
            hold_peak_to_peak_c: 1.0,
            sample_count: 12,
            stop_reason: "completed",
        }];
        let preview = vec![ThermalStageResult {
            target_temp_c: 120,
            rise_time_ms: 119_000,
            max_overshoot_c: 1.0,
            hold_peak_to_peak_c: 1.0,
            sample_count: 12,
            stop_reason: "timeout",
        }];

        let validation = validate_thermal_preview_results(&baseline, &preview);
        assert_eq!(validation["passed"], false);
        assert_eq!(validation["failures"][0]["reason"], "incomplete_stage");
    }

    #[test]
    fn thermal_hold_tracker_resets_when_temperature_leaves_stable_band() {
        let start = tokio::time::Instant::now();
        let mut tracker = ThermalHoldTracker::new(120, Duration::from_secs(10));

        assert_eq!(
            tracker.observe(119.6, 1_000, start),
            ThermalHoldObservation::Hold
        );
        assert_eq!(
            tracker.observe(119.8, 5_000, start + Duration::from_secs(5)),
            ThermalHoldObservation::Hold
        );
        assert_eq!(
            tracker.observe(110.0, 6_000, start + Duration::from_secs(6)),
            ThermalHoldObservation::Warmup
        );
        assert_eq!(tracker.rise_time_ms(), Some(1_000));
        assert!(tracker.peak_to_peak_c().is_infinite());
        assert_eq!(
            tracker.observe(119.7, 7_000, start + Duration::from_secs(7)),
            ThermalHoldObservation::Hold
        );
        assert_eq!(
            tracker.observe(119.8, 16_000, start + Duration::from_secs(16)),
            ThermalHoldObservation::Hold
        );
        assert_eq!(
            tracker.observe(119.9, 18_000, start + Duration::from_secs(18)),
            ThermalHoldObservation::Completed
        );
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
    fn isolapurr_power_config_matchers_validate_manual_target_and_auto_mode() {
        let config = json!({
            "tpsMode": "autoFollow",
            "manual": {
                "voltageMv": 20_000,
                "currentLimitMa": 3_250,
                "usbCPathMode": "force",
            }
        });

        assert!(isolapurr_power_config_value_matches_manual(
            &config, 20_000, 3_250
        ));
        assert!(isolapurr_power_config_value_is_auto(&config));
        assert!(!isolapurr_power_config_value_matches_manual(
            &config, 20_000, 3_000
        ));
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
            "--dry-run",
            "--json",
        ])
        .unwrap();

        match cli.command {
            Command::Thermal {
                command: ThermalCommand::SelfTest(args),
            } => {
                assert_eq!(args.target.device.as_deref(), Some("bench"));
                assert_eq!(args.source_device_id, "iso-1");
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
