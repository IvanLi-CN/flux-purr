use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use flux_purr_firmware::{
    control_plane::ThermalControlProfileWire,
    memory::ThermalControlProfileConfig,
    thermal_tuning::{
        THERMAL_TUNING_EVENT_CAPACITY, THERMAL_TUNING_EVENT_PAGE_SIZE,
        THERMAL_TUNING_EXECUTION_ORDER_C, THERMAL_TUNING_TARGET_BUDGET_MS,
        THERMAL_TUNING_TARGETS_C, ThermalTuningEvent, digest_event_chain, hash_profile,
    },
    thermal_tuning_limits::{ThermalTuningTimingEvidence, original_settle_limit_ms},
    thermal_tuning_measurement::{
        QualificationContinuity, QualificationMeasurement, QualificationObservation,
    },
};
use reqwest::{Client, Method};
use serde_json::{Value, json};

use super::{
    ResolvedUsbTarget, ThermalCandidateProfile, ThermalFirmwareSaveArgs, ThermalFlagshipTuneArgs,
    ThermalProfileMode, ThermalTuneEngine, create_lease, current_unix_millis,
    parse_thermal_targets, release_lease, request_leased, resolve_target, spawn_heartbeat,
    thermal_candidate_point, thermal_candidate_profile_from_value,
    thermal_candidate_profile_to_value, thermal_interpolated_candidate_point,
    thermal_tuning_v2_report::{self, ThermalTuningV2ReportInput},
};

#[cfg(test)]
use super::load_thermal_default_seed_candidate_profile;

const FIRMWARE_PHYSICAL_TARGETS_C: &str = "60,80,100,120,140,160,180,220";
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const TERMINAL_DRAIN_TIMEOUT: Duration = Duration::from_secs(120);
const RUNTIME_AUDIT_INTERVAL: Duration = Duration::from_secs(5);

struct StatusJournal {
    file: File,
    runtime_file: File,
    last_profile_hash: Option<String>,
}

impl StatusJournal {
    fn create(input_dir: &Path) -> io::Result<Self> {
        Ok(Self {
            file: fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(input_dir.join("device-statuses.ndjson"))?,
            runtime_file: fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(input_dir.join("device-runtime-statuses.ndjson"))?,
            last_profile_hash: None,
        })
    }

    fn resume(input_dir: &Path, statuses: &[Value]) -> io::Result<Self> {
        let last_profile_hash = statuses.iter().rev().find_map(|status| {
            status
                .get("candidateProfile")
                .filter(|profile| !profile.is_null())
                .and_then(|_| status.get("candidateHash"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        Ok(Self {
            file: fs::OpenOptions::new()
                .append(true)
                .open(input_dir.join("device-statuses.ndjson"))?,
            runtime_file: fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(input_dir.join("device-runtime-statuses.ndjson"))?,
            last_profile_hash,
        })
    }

    fn record(&mut self, statuses: &mut Vec<Value>, mut status: Value) -> io::Result<()> {
        if let Some(first) = statuses.first()
            && ["runId", "candidateId", "powerClass", "targetsC"]
                .iter()
                .any(|field| status[*field] != first[*field])
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "tuning_run_lost: device run identity or scope changed; retained evidence remains bound to the original run",
            ));
        }
        if !status_has_new_evidence(statuses.last(), &status, self.last_profile_hash.as_deref()) {
            return Ok(());
        }
        let candidate_hash = status.get("candidateHash").and_then(Value::as_str);
        let new_profile_hash = if status
            .get("candidateProfile")
            .is_some_and(|value| !value.is_null())
        {
            candidate_hash.map(str::to_owned)
        } else {
            None
        };
        if candidate_hash == self.last_profile_hash.as_deref()
            && let Some(object) = status.as_object_mut()
        {
            object.remove("candidateProfile");
        }
        serde_json::to_writer(&mut self.file, &status).map_err(io::Error::other)?;
        self.file.write_all(b"\n")?;
        self.file.sync_data()?;
        if let Some(hash) = new_profile_hash {
            self.last_profile_hash = Some(hash);
        }
        statuses.push(status);
        Ok(())
    }

    fn record_runtime(
        &mut self,
        run_id: &str,
        after_sequence: Option<u64>,
        runtime: &Value,
    ) -> io::Result<()> {
        serde_json::to_writer(
            &mut self.runtime_file,
            &json!({
                "runId": run_id, "afterSequence": after_sequence,
                "receivedAtUnixMs": current_unix_millis(), "status": runtime
            }),
        )
        .map_err(io::Error::other)?;
        self.runtime_file.write_all(b"\n")?;
        self.runtime_file.sync_data()
    }
}

fn status_has_new_evidence(
    previous: Option<&Value>,
    current: &Value,
    last_profile_hash: Option<&str>,
) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    if current
        .pointer("/events/events")
        .and_then(Value::as_array)
        .is_some_and(|events| !events.is_empty())
    {
        return true;
    }
    if current
        .get("candidateProfile")
        .is_some_and(|profile| !profile.is_null())
        && current.get("candidateHash").and_then(Value::as_str) != last_profile_hash
    {
        return true;
    }
    for pointer in [
        "/trace/lastSequence",
        "/trace/digest",
        "/trace/complete",
        "/trace/gap",
        "/candidateHash",
        "/phase",
        "/terminalDisposition",
        "/reviewDisposition",
        "/previewReceipt",
        "/saveReceipt",
    ] {
        if current.pointer(pointer) != previous.pointer(pointer) {
            return true;
        }
    }
    if current.get("terminalDisposition").and_then(Value::as_str) != Some("running")
        && current.pointer("/events/nextSequence") != previous.pointer("/events/nextSequence")
    {
        return true;
    }
    false
}

pub(crate) async fn run_firmware_tuning(
    client: &Client,
    default_devd: &str,
    args: ThermalFlagshipTuneArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    run_firmware_tuning_until_interrupt(client, default_devd, args, tokio::signal::ctrl_c()).await
}

async fn run_firmware_tuning_until_interrupt(
    client: &Client,
    default_devd: &str,
    args: ThermalFlagshipTuneArgs,
    interrupt: impl std::future::Future<Output = io::Result<()>>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let selected_targets = validate_firmware_args(&args)?;
    let execution_order = selected_execution_order(&selected_targets);
    let resolved = resolve_target(args.target.clone(), default_devd)?;
    let power_class = if args.profile_mode == ThermalProfileMode::W65 {
        "pps3a"
    } else {
        "pps5a"
    };
    if args.dry_run {
        return Ok(json!({
            "ok": true,
            "engine": "firmware",
            "powerClass": power_class,
            "targetsC": execution_order,
            "diagnosticOnly": selected_targets.len() != THERMAL_TUNING_TARGETS_C.len(),
            "device": resolved.device,
            "dryRun": true
        }));
    }
    let run_id = format!("firmware-{power_class}-{}", current_unix_millis());
    let candidate_id = format!("{run_id}-candidate");
    let candidate_profile = if let Some(path) = args.seed_profile_file.as_ref() {
        let imported: Value = serde_json::from_slice(&fs::read(path)?)?;
        parse_firmware_seed_profile(imported, power_class)?
    } else {
        Value::Null
    };
    let candidate_hash = if candidate_profile.is_null() {
        "unprepared".to_owned()
    } else {
        firmware_candidate_profile_hash(&candidate_profile)?
    };
    let input_dir = args.output_root.join(&run_id);
    fs::create_dir_all(&input_dir)?;
    let journal = StatusJournal::create(&input_dir)?;
    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let mut run = FirmwareRun {
        client,
        resolved: &resolved,
        lease_id: &lease.lease_id,
        args: &args,
        input_dir: &input_dir,
        run_id: &run_id,
        candidate_id: &candidate_id,
        power_class,
        selected_targets: &selected_targets,
        execution_order: &execution_order,
        candidate_profile,
        candidate_hash,
        journal,
        statuses: Vec::new(),
        identity_prepared: false,
        profile_mode_prepared: false,
        device_start_attempted: false,
    };
    let mut interruption_reason = "transport_or_runtime_failure";
    let result = tokio::select! {
        result = run.execute() => result,
        signal = interrupt => {
            interruption_reason = "host_canceled";
            Err(match signal {
                Ok(()) => "firmware thermal tuning interrupted; cancel and trace drain requested".into(),
                Err(error) => Box::new(error) as Box<dyn std::error::Error + Send + Sync>,
            })
        }
    };
    let rendered = run.recover_and_render(result, interruption_reason).await;
    let release_result = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();
    release_result?;
    rendered
}

enum FirmwareStart {
    Started(Value),
    HostFailure(Value),
}

struct FirmwareRun<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease_id: &'a str,
    args: &'a ThermalFlagshipTuneArgs,
    input_dir: &'a Path,
    run_id: &'a str,
    candidate_id: &'a str,
    power_class: &'a str,
    selected_targets: &'a [i16],
    execution_order: &'a [i16],
    candidate_profile: Value,
    candidate_hash: String,
    journal: StatusJournal,
    statuses: Vec<Value>,
    identity_prepared: bool,
    profile_mode_prepared: bool,
    device_start_attempted: bool,
}

impl FirmwareRun<'_> {
    async fn execute(&mut self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        self.prepare().await?;
        let start = match self.start().await? {
            FirmwareStart::Started(start) => start,
            FirmwareStart::HostFailure(rendered) => return Ok(rendered),
        };
        self.collect(start).await?;
        self.render()
    }

    async fn prepare(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let identity = request_leased(
            self.client,
            self.resolved,
            self.lease_id,
            Method::GET,
            "/identity",
            None,
        )
        .await?;
        record_device_identity(self.input_dir, self.run_id, &identity)?;
        self.identity_prepared = true;
        let runtime = ensure_firmware_profile_mode(
            self.client,
            self.resolved,
            self.lease_id,
            self.power_class,
        )
        .await?;
        self.journal.record_runtime(self.run_id, None, &runtime)?;
        self.profile_mode_prepared = true;
        if self.candidate_profile.is_null() {
            let calibration = request_leased(
                self.client,
                self.resolved,
                self.lease_id,
                Method::GET,
                "/calibration/thermal-plant/run",
                None,
            )
            .await?;
            self.candidate_profile = calibrated_firmware_seed_profile(&runtime, &calibration)?;
            self.candidate_hash = firmware_candidate_profile_hash(&self.candidate_profile)?;
            fs::write(
                self.input_dir.join("seed-calibration.json"),
                serde_json::to_vec_pretty(&json!({
                    "source": "device_active_calibration",
                    "runId": self.run_id,
                    "powerClass": self.power_class,
                    "thermalPlantModel": runtime["thermalPlantModel"],
                    "ppsCapabilityMaxMv": runtime["ppsCapabilityMaxMv"],
                    "ppsCapabilityMaxMa": runtime["ppsCapabilityMaxMa"],
                    "calibration": calibration,
                    "candidateHash": self.candidate_hash,
                    "candidateProfile": self.candidate_profile,
                }))?,
            )?;
        }
        Ok(())
    }

    async fn start(&mut self) -> Result<FirmwareStart, Box<dyn std::error::Error + Send + Sync>> {
        self.device_start_attempted = true;
        let start = match request_leased(
            self.client,
            self.resolved,
            self.lease_id,
            Method::POST,
            "/thermal-tuning",
            Some(firmware_start_command(
                self.power_class,
                self.run_id,
                self.candidate_id,
                &self.candidate_hash,
                &self.candidate_profile,
                self.execution_order,
            )),
        )
        .await
        {
            Ok(start) => start,
            Err(error) => {
                eprintln!("firmware thermal tuning start failed: {error}");
                let rejected = request_leased(
                    self.client,
                    self.resolved,
                    self.lease_id,
                    Method::POST,
                    "/thermal-tuning",
                    Some(firmware_read_command(self.run_id, None)),
                )
                .await;
                match rejected {
                    Ok(status)
                        if is_confirmed_rejected_start(
                            self.power_class,
                            &status,
                            self.run_id,
                            self.candidate_id,
                            &self.candidate_hash,
                        ) =>
                    {
                        status
                    }
                    _ => {
                        let _ = request_leased(
                            self.client,
                            self.resolved,
                            self.lease_id,
                            Method::POST,
                            "/thermal-tuning",
                            Some(json!({"op": "cancel"})),
                        )
                        .await;
                        return Ok(FirmwareStart::HostFailure(render_host_failure(
                            HostFailureInput {
                                device_id: &self.resolved.device,
                                output_root: &self.args.output_root,
                                bundle_dir: self.args.bundle_dir.clone(),
                                run_id: Some(self.run_id),
                                power_class: self.power_class,
                                reason: "start_not_confirmed",
                                no_device_command: false,
                                attempted_candidate: Some((
                                    self.candidate_id,
                                    &self.candidate_hash,
                                    &self.candidate_profile,
                                )),
                                selected_targets: Some(self.selected_targets),
                            },
                        )?));
                    }
                }
            }
        };

        Ok(FirmwareStart::Started(start))
    }

    async fn collect(
        &mut self,
        start: Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let deadline = Instant::now() + firmware_host_watchdog(self.selected_targets.len());
        let mut next_runtime_audit = Instant::now();
        let mut after_sequence = next_event_sequence(&start);
        self.journal.record(&mut self.statuses, start)?;
        loop {
            let status = request_firmware_event_read(
                self.client,
                self.resolved,
                self.lease_id,
                self.run_id,
                after_sequence,
            )
            .await
            .map_err(|error| {
                eprintln!("firmware thermal tuning event read failed: {error}");
                error
            })?;
            after_sequence = next_event_sequence(&status).max(after_sequence);
            let caught_up = event_stream_caught_up(&status, after_sequence);
            let terminal = status
                .get("terminalDisposition")
                .and_then(Value::as_str)
                .is_some_and(|value| {
                    matches!(value, "candidate_ready" | "passed" | "failed" | "canceled")
                });
            let trace_gap = status_trace_gap(&status);
            self.journal.record(&mut self.statuses, status)?;
            if terminal
                && caught_up
                && !trace_gap
                && let Some(sequence) = after_sequence
            {
                let confirmed = confirm_terminal_tuning_events(
                    self.client,
                    self.resolved,
                    self.lease_id,
                    self.run_id,
                    sequence,
                )
                .await?;
                self.journal.record(&mut self.statuses, confirmed)?;
            }
            if terminal && caught_up {
                break;
            }
            if caught_up && Instant::now() >= next_runtime_audit {
                let runtime = request_leased(
                    self.client,
                    self.resolved,
                    self.lease_id,
                    Method::GET,
                    "/status",
                    None,
                )
                .await?;
                self.journal
                    .record_runtime(self.run_id, after_sequence, &runtime)?;
                eprintln!(
                    "firmware tuning: temperature={}C heater={}%, fan command enabled={} level={} pwm={}permille",
                    runtime["currentTempC"],
                    runtime["heaterPhysicalOutputPercent"],
                    runtime["fanEnabled"],
                    runtime["fanOutputLevel"],
                    runtime["fanPwmPermille"]
                );
                next_runtime_audit = Instant::now() + RUNTIME_AUDIT_INTERVAL;
            }
            if Instant::now() >= deadline {
                return Err("firmware thermal tuning exceeded the bounded run budget".into());
            }
            if caught_up {
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }

        Ok(())
    }

    fn render(&self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let candidate_hash = self
            .statuses
            .last()
            .and_then(|status| status.get("candidateHash"))
            .and_then(Value::as_str)
            .unwrap_or(&self.candidate_hash)
            .to_string();

        write_v2_input(V2Input {
            input_dir: self.input_dir,
            device_id: &self.resolved.device,
            run_id: self.run_id,
            candidate_id: self.candidate_id,
            candidate_hash: &candidate_hash,
            candidate_profile: &self.candidate_profile,
            statuses: &self.statuses,
            host_interruption: None,
            selected_targets: self.selected_targets,
            execution_order: self.execution_order,
        })?;
        let mut rendered =
            thermal_tuning_v2_report::render_v2_bundle(ThermalTuningV2ReportInput {
                input_dir: self.input_dir.to_path_buf(),
                output_dir: self.args.bundle_dir.clone(),
            })?;
        rendered["rawInputDir"] = json!(self.input_dir);
        Ok(rendered)
    }

    async fn recover_and_render(
        &mut self,
        result: Result<Value, Box<dyn std::error::Error + Send + Sync>>,
        interruption_reason: &str,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        if let Err(error) = &result {
            eprintln!("firmware tuning failed: {error}");
        }
        if result.is_err() && self.device_start_attempted {
            match tokio::time::timeout(
                TERMINAL_DRAIN_TIMEOUT,
                cancel_and_drain_firmware_run(
                    self.client,
                    self.resolved,
                    self.lease_id,
                    self.run_id,
                    &mut self.journal,
                    &mut self.statuses,
                ),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("thermal tuning cancel/trace recovery failed: {error}"),
                Err(_) => {
                    eprintln!("thermal tuning cancel/trace recovery exceeded its bounded deadline")
                }
            }
        }
        match result {
            Ok(rendered) => Ok(rendered),
            Err(_) if !self.statuses.is_empty() => render_interrupted_run(InterruptedRunInput {
                device_id: &self.resolved.device,
                output_root: &self.args.output_root,
                bundle_dir: self.args.bundle_dir.clone(),
                run_id: self.run_id,
                candidate_id: self.candidate_id,
                seed_candidate_hash: &self.candidate_hash,
                candidate_profile: &self.candidate_profile,
                statuses: &self.statuses,
                selected_targets: self.selected_targets,
                execution_order: self.execution_order,
                reason: interruption_reason,
            }),
            Err(_) => render_host_failure(HostFailureInput {
                device_id: &self.resolved.device,
                output_root: &self.args.output_root,
                bundle_dir: self.args.bundle_dir.clone(),
                run_id: Some(self.run_id),
                power_class: self.power_class,
                reason: if self.device_start_attempted {
                    "transport_or_runtime_failure"
                } else if self.profile_mode_prepared {
                    "calibration_seed_unavailable"
                } else if !self.identity_prepared {
                    "device_identity_not_confirmed"
                } else {
                    "profile_mode_not_confirmed"
                },
                no_device_command: !self.device_start_attempted,
                attempted_candidate: (!self.candidate_profile.is_null()).then_some((
                    self.candidate_id,
                    &self.candidate_hash,
                    &self.candidate_profile,
                )),
                selected_targets: Some(self.selected_targets),
            }),
        }
    }
}

async fn cancel_and_drain_firmware_run(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    run_id: &str,
    journal: &mut StatusJournal,
    statuses: &mut Vec<Value>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // A cancel response can repeat the last delivered page. It does not imply
    // that the retained tail (including the safety event) has been collected.
    let canceled = request_leased(
        client,
        resolved,
        lease_id,
        Method::POST,
        "/thermal-tuning",
        Some(json!({"op": "cancel"})),
    )
    .await?;
    require_disarmed_run(&canceled, run_id)?;
    journal.record(statuses, canceled)?;
    let mut after_sequence = statuses.iter().filter_map(next_event_sequence).max();
    let max_pages = THERMAL_TUNING_EVENT_CAPACITY.div_ceil(THERMAL_TUNING_EVENT_PAGE_SIZE) + 1;
    for _ in 0..max_pages {
        let previous = after_sequence;
        let status =
            request_firmware_event_read(client, resolved, lease_id, run_id, after_sequence).await?;
        require_disarmed_run(&status, run_id)?;
        let received = next_event_sequence(&status).max(after_sequence);
        let caught_up = event_stream_caught_up(&status, received);
        let gap = status_trace_gap(&status);
        // Advance the device ACK only after this page has reached durable storage.
        journal.record(statuses, status)?;
        after_sequence = received;
        if caught_up {
            if !gap && let Some(sequence) = after_sequence {
                let confirmed =
                    confirm_terminal_tuning_events(client, resolved, lease_id, run_id, sequence)
                        .await?;
                require_disarmed_run(&confirmed, run_id)?;
                journal.record(statuses, confirmed)?;
            }
            return Ok(());
        }
        if after_sequence == previous {
            return Err("terminal tuning trace recovery made no progress".into());
        }
    }
    Err("terminal tuning trace recovery exceeded the retained event capacity".into())
}

fn require_disarmed_run(
    status: &Value,
    run_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if status.get("runId").and_then(Value::as_str) != Some(run_id)
        || status.get("armed").and_then(Value::as_bool) != Some(false)
        || !status
            .get("terminalDisposition")
            .and_then(Value::as_str)
            .is_some_and(|terminal| {
                matches!(
                    terminal,
                    "canceled" | "failed" | "candidate_ready" | "passed"
                )
            })
    {
        return Err("terminal tuning recovery did not confirm the same disarmed run".into());
    }
    Ok(())
}

fn firmware_profile_mode_ready(status: &Value, power_class: &str) -> bool {
    status.get("thermalProfileMode").and_then(Value::as_str)
        == Some(firmware_profile_mode(power_class))
        && status
            .get("thermalProfileResolvedBank")
            .and_then(Value::as_str)
            == Some(power_class)
}

fn firmware_profile_mode(power_class: &str) -> &'static str {
    if power_class == "pps3a" {
        "65w"
    } else {
        "100w"
    }
}

fn require_firmware_automatic_power(
    status: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if status.get("manualPpsEnabled").and_then(Value::as_bool) != Some(false) {
        return Err(
            "firmware tuning requires manualPpsEnabled=false; stop heating and clear the manual PPS override before starting"
                .into(),
        );
    }
    Ok(())
}

async fn ensure_firmware_profile_mode(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    power_class: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let status = request_leased(client, resolved, lease_id, Method::GET, "/status", None).await?;
    require_firmware_automatic_power(&status)?;
    if firmware_profile_mode_ready(&status, power_class) {
        return Ok(status);
    }
    if status.get("heaterEnabled").and_then(Value::as_bool) != Some(false) {
        return Err("heater must be off before selecting the requested profile mode".into());
    }
    request_leased(
        client,
        resolved,
        lease_id,
        Method::PUT,
        "/runtime",
        Some(json!({"thermalProfileMode": firmware_profile_mode(power_class)})),
    )
    .await?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status =
            request_leased(client, resolved, lease_id, Method::GET, "/status", None).await?;
        require_firmware_automatic_power(&status)?;
        if firmware_profile_mode_ready(&status, power_class) {
            return Ok(status);
        }
        if status.get("heaterEnabled").and_then(Value::as_bool) != Some(false) {
            return Err("heater became armed while selecting the requested profile mode".into());
        }
        if Instant::now() >= deadline {
            return Err("profile mode readback timed out".into());
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

pub(crate) async fn confirm_firmware_save(
    client: &Client,
    default_devd: &str,
    args: ThermalFirmwareSaveArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let bundle: Value = serde_json::from_slice(&fs::read(args.input_dir.join("run.bundle.json"))?)?;
    let candidate: Value = serde_json::from_slice(&fs::read(
        args.input_dir.join("thermal-profile.candidate.json"),
    )?)?;
    let mut statuses: Vec<Value> =
        serde_json::from_slice(&fs::read(args.input_dir.join("device-statuses.json"))?)?;
    let mut journal = StatusJournal::resume(&args.input_dir, &statuses)?;
    let run_id = required_string(&bundle, "runId")?;
    let candidate_id = required_string(&candidate, "candidateId")?;
    let candidate_hash = required_string(&candidate, "candidateHash")?;
    let power_class = required_power_class(&bundle)?;
    validate_save_input(&bundle, &candidate)?;
    let resolved = resolve_target(args.target, default_devd)?;
    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let result = async {
        let last_sequence = statuses
            .last()
            .and_then(|status| status.pointer("/trace/lastSequence"))
            .and_then(Value::as_u64);
        let current =
            request_firmware_event_read(client, &resolved, &lease.lease_id, run_id, last_sequence)
                .await?;
        require_save_candidate_identity(
            &current,
            [run_id, candidate_id, candidate_hash, power_class],
        )?;
        journal.record(&mut statuses, current)?;
        seal_firmware_candidate(
            client,
            &resolved,
            &lease.lease_id,
            run_id,
            &mut journal,
            &mut statuses,
        )
        .await?;
        let saved = request_leased(
            client,
            &resolved,
            &lease.lease_id,
            Method::POST,
            "/thermal-tuning",
            Some(json!({
                "op": "confirm_save",
                "runId": run_id,
                "candidateId": candidate_id,
                "candidateHash": candidate_hash,
                "powerClass": power_class
            })),
        )
        .await?;
        if saved.get("terminalDisposition").and_then(Value::as_str) != Some("passed")
            || saved.get("saveReceipt").is_none_or(Value::is_null)
        {
            return Err("device did not return a verified save receipt".into());
        }
        journal.record(&mut statuses, saved)?;
        acknowledge_saved_candidate(
            client,
            &resolved,
            &lease.lease_id,
            run_id,
            &mut journal,
            &mut statuses,
        )
        .await?;
        let candidate_profile = candidate
            .pointer("/profile/value")
            .ok_or("candidate profile missing from input")?;
        write_v2_input(V2Input {
            input_dir: &args.input_dir,
            device_id: &resolved.device,
            run_id,
            candidate_id,
            candidate_hash,
            candidate_profile,
            statuses: &statuses,
            host_interruption: None,
            selected_targets: &THERMAL_TUNING_TARGETS_C,
            execution_order: &THERMAL_TUNING_EXECUTION_ORDER_C,
        })?;
        let mut rendered =
            thermal_tuning_v2_report::render_v2_bundle(ThermalTuningV2ReportInput {
                input_dir: args.input_dir.clone(),
                output_dir: args.output_dir,
            })?;
        rendered["rawInputDir"] = json!(args.input_dir);
        Ok(rendered)
    }
    .await;
    let release_result = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();
    release_result?;
    result
}

async fn acknowledge_saved_candidate(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    run_id: &str,
    journal: &mut StatusJournal,
    statuses: &mut Vec<Value>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(sequence) = statuses.last().and_then(next_event_sequence) {
        let confirmed =
            request_firmware_event_read(client, resolved, lease_id, run_id, Some(sequence)).await?;
        if confirmed.pointer("/trace/gap").and_then(Value::as_bool) != Some(false)
            || confirmed
                .pointer("/trace/lastSequence")
                .and_then(Value::as_u64)
                != Some(sequence)
        {
            return Err("saved tuning receipt event was not confirmed".into());
        }
        journal.record(statuses, confirmed)?;
    }
    Ok(())
}

fn require_save_candidate_identity(
    current: &Value,
    identity: [&str; 4],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let [run_id, candidate_id, candidate_hash, power_class] = identity;
    if current.get("runId").and_then(Value::as_str) != Some(run_id)
        || current.get("candidateId").and_then(Value::as_str) != Some(candidate_id)
        || current.get("candidateHash").and_then(Value::as_str) != Some(candidate_hash)
        || current.get("powerClass").and_then(Value::as_str) != Some(power_class)
        || current.get("terminalDisposition").and_then(Value::as_str) != Some("candidate_ready")
        || current.pointer("/trace/gap").and_then(Value::as_bool) != Some(false)
    {
        return Err("device candidate identity or trace changed before confirmation".into());
    }
    Ok(())
}

async fn seal_firmware_candidate(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    run_id: &str,
    journal: &mut StatusJournal,
    statuses: &mut Vec<Value>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sealed = request_leased(
        client,
        resolved,
        lease_id,
        Method::POST,
        "/thermal-tuning",
        Some(json!({"op": "seal"})),
    )
    .await?;
    let seal_sequence = next_event_sequence(&sealed)
        .ok_or("sealed candidate did not emit an auditable transition")?;
    journal.record(statuses, sealed)?;
    let seal_ack =
        request_firmware_event_read(client, resolved, lease_id, run_id, Some(seal_sequence))
            .await?;
    if seal_ack.pointer("/trace/gap").and_then(Value::as_bool) != Some(false)
        || seal_ack
            .pointer("/trace/lastSequence")
            .and_then(Value::as_u64)
            != Some(seal_sequence)
    {
        return Err("sealed candidate transition was not acknowledged".into());
    }
    journal.record(statuses, seal_ack)?;
    Ok(())
}

fn validate_save_input(
    bundle: &Value,
    candidate: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let run_id = required_string(bundle, "runId")?;
    let candidate_id = required_string(candidate, "candidateId")?;
    let candidate_hash = required_string(candidate, "candidateHash")?;
    let power_class = required_power_class(bundle)?;
    if !default_scope_passed(bundle)
        || bundle.get("terminalDisposition").and_then(Value::as_str) != Some("candidate_ready")
        || bundle
            .pointer("/deviceTraceVerification/verified")
            .and_then(Value::as_bool)
            != Some(true)
        || candidate.get("runId").and_then(Value::as_str) != Some(run_id)
        || candidate.pointer("/profile/bank").and_then(Value::as_str) != Some(power_class)
        || candidate.get("powerClass").and_then(Value::as_str) != Some(power_class)
        || candidate
            .pointer("/promotion/previewReceipt/powerClass")
            .and_then(Value::as_str)
            != Some(power_class)
        || bundle
            .pointer("/candidate/candidateId")
            .and_then(Value::as_str)
            != Some(candidate_id)
        || bundle
            .pointer("/candidate/candidateHash")
            .and_then(Value::as_str)
            != Some(candidate_hash)
        || candidate
            .pointer("/promotion/previewReceipt/runId")
            .and_then(Value::as_str)
            != Some(run_id)
        || candidate
            .pointer("/promotion/previewReceipt/candidateId")
            .and_then(Value::as_str)
            != Some(candidate_id)
        || candidate
            .pointer("/promotion/previewReceipt/candidateHash")
            .and_then(Value::as_str)
            != Some(candidate_hash)
        || candidate
            .pointer("/promotion/previewReceipt/ramAppliedHash")
            .and_then(Value::as_str)
            != Some(candidate_hash)
        || candidate
            .pointer("/promotion/saveReceipt")
            .is_some_and(|value| !value.is_null())
    {
        return Err("input is not a verified, sealed firmware candidate".into());
    }
    Ok(())
}

fn required_power_class(value: &Value) -> Result<&str, Box<dyn std::error::Error + Send + Sync>> {
    let power_class = required_string(value, "powerClass")?;
    if !matches!(power_class, "pps3a" | "pps5a") {
        return Err("unsupported thermal power class".into());
    }
    Ok(power_class)
}

fn default_scope_passed(bundle: &Value) -> bool {
    let Ok(power_class) = required_power_class(bundle) else {
        return false;
    };
    let Ok(physical) = parse_thermal_targets(Some(FIRMWARE_PHYSICAL_TARGETS_C)) else {
        return false;
    };
    if bundle.get("schema").and_then(Value::as_str) != Some("thermal-tuning-v2")
        || bundle.get("engine").and_then(Value::as_str) != Some("firmware")
        || bundle.pointer("/targetSet/physicalC") != Some(&json!(physical))
        || bundle.pointer("/targetSet/selectedC") != Some(&json!(physical))
        || bundle.pointer("/targetSet/executionOrderC")
            != Some(&json!(THERMAL_TUNING_EXECUTION_ORDER_C))
        || bundle
            .pointer("/qualificationScope/kind")
            .and_then(Value::as_str)
            != Some(format!("default-{power_class}-full-batch").as_str())
        || bundle
            .pointer("/qualificationScope/diagnosticOnly")
            .and_then(Value::as_bool)
            != Some(false)
        || bundle
            .get("hostInterruption")
            .is_some_and(|value| !value.is_null())
    {
        return false;
    }
    let Some(runs) = bundle.get("reportRuns").and_then(Value::as_array) else {
        return false;
    };
    let mut accepted = BTreeSet::new();
    for run in runs {
        let Some(target_c) = run.get("targetC").and_then(Value::as_i64) else {
            return false;
        };
        let Some(adopted_index) = run.get("adoptedTrialIndex").and_then(Value::as_i64) else {
            return false;
        };
        let adopted = run
            .get("trials")
            .and_then(Value::as_array)
            .and_then(|trials| {
                trials.iter().find(|trial| {
                    trial.get("trialIndex").and_then(Value::as_i64) == Some(adopted_index)
                        && trial.get("disposition").and_then(Value::as_str) == Some("adopted")
                })
            });
        if !physical.iter().any(|target| i64::from(*target) == target_c)
            || !accepted.insert(target_c)
            || run.get("reviewOutcome").and_then(Value::as_str) != Some("passed")
            || run.get("reviewPassed").and_then(Value::as_bool) != Some(true)
            || adopted.is_none_or(|trial| !adopted_trial_passes_gates(trial))
        {
            return false;
        }
    }
    accepted.len() == physical.len()
}

fn adopted_trial_passes_gates(trial: &Value) -> bool {
    let stable = trial.get("fullSpeedToStable").unwrap_or(&Value::Null);
    let settle = stable.get("settleTimeMs").and_then(Value::as_u64);
    let limit = stable.get("limitMs").and_then(Value::as_u64);
    trial.get("trialPurpose").and_then(Value::as_str) == Some("hold_confirm")
        && timing_gate_valid(trial)
        && trial
            .get("warmupRequestedOutputPermille")
            .and_then(Value::as_u64)
            .is_some_and(|value| value >= 995)
        && trial
            .get("maxOvershootC")
            .and_then(Value::as_f64)
            .is_some_and(|value| value <= 3.0)
        && trial
            .get("holdPeakToPeakC")
            .and_then(Value::as_f64)
            .is_some_and(|value| value <= 3.0)
        && trial
            .pointer("/metrics/holdConfirmSeconds")
            .and_then(Value::as_f64)
            .is_some_and(|value| value >= 60.0)
        && trial
            .pointer("/holdEvidenceVerification/verified")
            .and_then(Value::as_bool)
            == Some(true)
        && matches!((settle, limit), (Some(settle), Some(limit)) if settle <= limit)
        && trial
            .pointer("/metrics/phasePath")
            .and_then(Value::as_str)
            .is_some_and(|path| path.contains("approach") && path.contains("hold"))
}

fn timing_gate_valid(trial: &Value) -> bool {
    let Some(value) = trial.get("timing").filter(|value| !value.is_null()) else {
        // Retain the device disposition, but do not qualify absent policy facts.
        return false;
    };
    let Ok(timing) = serde_json::from_value::<ThermalTuningTimingEvidence>(value.clone()) else {
        return false;
    };
    let Some(target) = trial
        .get("targetC")
        .and_then(Value::as_i64)
        .and_then(|target| i16::try_from(target).ok())
    else {
        return false;
    };
    let Some(point) = trial.pointer("/canonicalCandidate/point") else {
        return false;
    };
    let Some(floor) = point
        .get("approachFloorPowerPermille")
        .and_then(Value::as_u64)
    else {
        return false;
    };
    let Some(hold) = point.get("holdPowerPermille").and_then(Value::as_u64) else {
        return false;
    };
    let settle = timing
        .warmup_exited_at_ms
        .zip(timing.stable_window_started_at_ms)
        .and_then(|(exit, start)| start.checked_sub(exit));
    timing.supported_policy()
        && timing.original_limit_ms == original_settle_limit_ms(target)
        && Some(timing.extension_limit_ms) == timing.original_limit_ms.checked_mul(3)
        && floor <= 1_000
        && hold <= 1_000
        && timing.configured_approach_floor_permille as u64 == floor
        && timing.effective_approach_floor_permille as u64
            == if timing.model_approach_floor_permille.is_some() {
                u64::from(timing.allowed_approach_floor_permille)
            } else {
                floor.max(hold)
            }
        && (timing.extended_at_ms.is_none() || timing.valid_extension())
        && trial
            .pointer("/timingEvidenceVerification/verified")
            .and_then(Value::as_bool)
            == Some(true)
        && trial
            .pointer("/fullSpeedToStable/limitMs")
            .and_then(Value::as_u64)
            == Some(u64::from(timing.active_limit_ms()))
        && trial
            .pointer("/fullSpeedToStable/settleTimeMs")
            .and_then(Value::as_u64)
            == settle
        && timing
            .stable_window_started_at_ms
            .zip(timing.stable_window_verified_at_ms)
            .is_some_and(|(start, verified)| {
                verified.checked_sub(start).is_some_and(|age| age >= 10_000)
            })
}

fn sample_measurement_accepted(sample: &Value) -> bool {
    sample.get("measurementValid").and_then(Value::as_bool) == Some(true)
        && sample.get("measurementAccepted").and_then(Value::as_bool) != Some(false)
}

fn qualification_measurements(
    timing: &ThermalTuningTimingEvidence,
    target: i16,
    samples: &[&Value],
) -> Vec<QualificationMeasurement> {
    let mut continuity = QualificationContinuity::new();
    samples
        .iter()
        .map(|sample| {
            if timing.policy_version != 5 {
                return if sample_measurement_accepted(sample) {
                    QualificationMeasurement::Fresh
                } else {
                    QualificationMeasurement::Rejected
                };
            }
            let temperature = |field| {
                sample
                    .get(field)
                    .and_then(Value::as_i64)
                    .and_then(|v| i16::try_from(v).ok())
            };
            continuity.observe(QualificationObservation {
                elapsed_ms: sample
                    .get("elapsedMs")
                    .and_then(Value::as_u64)
                    .unwrap_or(u64::MAX),
                requested_mv: sample
                    .get("ppsRequestedMv")
                    .and_then(Value::as_u64)
                    .and_then(|v| u16::try_from(v).ok()),
                confirmed_contract_mv: sample
                    .get("ppsContractConfirmed")
                    .and_then(Value::as_bool)
                    .filter(|confirmed| *confirmed)
                    .and_then(|_| sample.get("ppsContractMv"))
                    .and_then(Value::as_u64)
                    .and_then(|v| u16::try_from(v).ok()),
                valid: sample.get("measurementValid").and_then(Value::as_bool) == Some(true)
                    && temperature("temperatureCentiC").is_some(),
                accepted: sample_measurement_accepted(sample),
                temperature_centi_c: temperature("temperatureCentiC").unwrap_or(i16::MIN),
                raw_temperature_centi_c: temperature("rawTemperatureCentiC"),
                target_c: target,
            })
        })
        .collect()
}

fn verify_hold_samples(
    timing: Option<&ThermalTuningTimingEvidence>,
    target: i16,
    samples: &[&Value],
) -> Value {
    let Some(timing) = timing.filter(|timing| timing.supported_policy()) else {
        return json!({"verified": false, "source": "trial_samples",
            "continuousHoldMs": 0, "longestHoldMs": 0,
            "reasons": ["unsupported_timing_policy"]});
    };
    let measurements = qualification_measurements(timing, target, samples);
    let mut start = None;
    let mut last_fresh = None;
    let mut previous = None;
    let mut count = 0_u64;
    let mut duration = 0_u64;
    let mut longest = 0_u64;
    let mut ordered = true;
    for (sample, measurement) in samples.iter().zip(measurements) {
        let position = sample
            .get("sequence")
            .and_then(Value::as_u64)
            .zip(sample.get("elapsedMs").and_then(Value::as_u64));
        let Some((sequence, time)) = position else {
            ordered = false;
            break;
        };
        if previous.is_some_and(|(seq, at)| sequence <= seq || time <= at) {
            ordered = false;
            break;
        }
        previous = position;
        let in_hold = sample.get("phase").and_then(Value::as_str) == Some("hold");
        if !in_hold
            || measurement == QualificationMeasurement::Rejected
            || measurement == QualificationMeasurement::FreshAfterGap
            || last_fresh.is_some_and(|at| time.saturating_sub(at) > 1_000)
        {
            start = None;
            last_fresh = None;
            count = 0;
            duration = 0;
        }
        if !in_hold
            || measurement == QualificationMeasurement::Rejected
            || measurement == QualificationMeasurement::PpsPause
        {
            continue;
        }
        let Some(temperature) = sample
            .get("temperatureCentiC")
            .and_then(Value::as_i64)
            .and_then(|value| i16::try_from(value).ok())
        else {
            start = None;
            last_fresh = None;
            count = 0;
            duration = 0;
            continue;
        };
        if start.is_none()
            && flux_purr_firmware::heater_phase::hold_entry_ready(
                (i32::from(target) * 100 - i32::from(temperature)) as f32 / 100.0,
            )
        {
            start = Some(time);
        }
        if let Some(started) = start {
            last_fresh = Some(time);
            count += 1;
            duration = time - started;
            if count.saturating_sub(1) * 1_000 >= duration * 3 {
                longest = longest.max(duration);
            }
        }
    }
    let verified = ordered && duration >= 60_000 && count.saturating_sub(1) * 1_000 >= duration * 3;
    let reasons = if !ordered {
        vec!["invalid_sample_order"]
    } else if !verified {
        vec!["continuous_hold_not_verified_from_samples"]
    } else {
        vec![]
    };
    json!({"verified": verified, "source": "trial_samples",
        "continuousHoldMs": duration, "longestHoldMs": longest, "reasons": reasons})
}

fn verify_timing_samples(
    timing: &ThermalTuningTimingEvidence,
    target: i16,
    samples: &[&Value],
) -> Value {
    let mut reasons = Vec::new();
    let measurements = qualification_measurements(timing, target, samples);
    let deadline = timing
        .warmup_exited_at_ms
        .and_then(|exit| exit.checked_add(u64::from(timing.original_limit_ms)));
    if !first_deadline_matches(timing, samples, deadline) {
        reasons.push("first_original_deadline_check_mismatch");
    }
    if let Some(extended) = timing.extended_at_ms {
        let witness = samples.iter().find(|sample| {
            sample.get("elapsedMs").and_then(Value::as_u64) == timing.floor_observed_at_ms
        });
        if witness.is_none_or(|sample| {
            sample.get("phase").and_then(Value::as_str) != Some("approach")
                || sample
                    .get("heaterRequestedOutputPermille")
                    .and_then(Value::as_u64)
                    != Some(u64::from(timing.allowed_approach_floor_permille))
                || !sample_measurement_accepted(sample)
        }) {
            reasons.push(if timing.allowed_approach_floor_permille == 50 {
                "missing_real_50_permille_floor_sample"
            } else {
                "missing_real_100_permille_floor_sample"
            });
        }
        let mut stable_start = None;
        let mut last_time: Option<u64> = None;
        for (sample, measurement) in samples.iter().zip(&measurements) {
            let Some(time) = sample.get("elapsedMs").and_then(Value::as_u64) else {
                continue;
            };
            if time > extended {
                break;
            }
            if sample.get("phase").and_then(Value::as_str) == Some("approach")
                && timing
                    .floor_observed_at_ms
                    .is_some_and(|floor| time >= floor)
                && sample
                    .get("heaterRequestedOutputPermille")
                    .and_then(Value::as_u64)
                    .is_none_or(|request| {
                        request != 0 && request != u64::from(timing.allowed_approach_floor_permille)
                    })
            {
                reasons.push("floor_request_raised_before_deadline");
            }
            if timing.warmup_exited_at_ms.is_none_or(|exit| time < exit) {
                continue;
            }
            if *measurement == QualificationMeasurement::PpsPause {
                continue;
            }
            if last_time.is_some_and(|last| time.saturating_sub(last) > 1_000) {
                stable_start = None;
            }
            last_time = Some(time);
            if sample_measurement_accepted(sample)
                && sample
                    .get("temperatureCentiC")
                    .and_then(Value::as_i64)
                    .is_some_and(|temp| temp.abs_diff(i64::from(target) * 100) <= 150)
            {
                stable_start.get_or_insert(time);
            } else {
                stable_start = None;
            }
        }
        if stable_start
            .zip(deadline)
            .is_some_and(|(start, deadline)| start <= deadline)
        {
            reasons.push("timely_stable_window_at_original_deadline");
        }
        if !timing.valid_extension() {
            reasons.push("invalid_fixed_floor_extension_identity");
        }
    }
    if !stable_window_samples_valid(timing, target, samples) {
        reasons.push("stable_window_not_verified_from_samples");
    }
    json!({"verified": timing.supported_policy() && reasons.is_empty(),
        "source": "trial_samples", "reasons": reasons})
}

fn first_deadline_matches(
    timing: &ThermalTuningTimingEvidence,
    samples: &[&Value],
    deadline: Option<u64>,
) -> bool {
    let first_check = deadline.and_then(|deadline| {
        samples.iter().find(|sample| {
            sample
                .get("elapsedMs")
                .and_then(Value::as_u64)
                .is_some_and(|time| time >= deadline)
        })
    });
    let recorded_phase = timing
        .original_deadline_phase
        .and_then(|phase| serde_json::to_value(phase).ok());
    first_check
        .and_then(|sample| sample.get("elapsedMs"))
        .and_then(Value::as_u64)
        == timing.original_deadline_observed_at_ms
        && first_check
            .and_then(|sample| sample.get("phase"))
            .and_then(Value::as_str)
            == recorded_phase.as_ref().and_then(Value::as_str)
}

fn stable_window_samples_valid(
    timing: &ThermalTuningTimingEvidence,
    target: i16,
    samples: &[&Value],
) -> bool {
    let window = timing
        .stable_window_started_at_ms
        .zip(timing.stable_window_verified_at_ms);
    let mut window_times = Vec::new();
    let mut window_samples_valid = true;
    let measurements = qualification_measurements(timing, target, samples);
    if let Some((start, verified)) = window {
        for (sample, measurement) in samples.iter().zip(measurements) {
            let Some(time) = sample.get("elapsedMs").and_then(Value::as_u64) else {
                continue;
            };
            if time < start || time > verified {
                continue;
            }
            if measurement == QualificationMeasurement::PpsPause {
                // A pause preserves a provisional start but is not a witness.
                window_samples_valid &= time != start && time != verified;
                continue;
            }
            window_samples_valid &= sample_measurement_accepted(sample)
                && sample
                    .get("temperatureCentiC")
                    .and_then(Value::as_i64)
                    .is_some_and(|temp| temp.abs_diff(i64::from(target) * 100) <= 150);
            if window_times.last() != Some(&time) {
                window_times.push(time);
            }
        }
        window_samples_valid &= window_times.first() == Some(&start)
            && window_times.last() == Some(&verified)
            && verified.checked_sub(start).is_some_and(|duration| {
                duration >= 10_000
                    && (window_times.len().saturating_sub(1) as u64).saturating_mul(1_000)
                        >= duration.saturating_mul(3)
            })
            && window_times
                .windows(2)
                .all(|pair| pair[1].checked_sub(pair[0]).is_some_and(|gap| gap <= 1_000));
    } else {
        window_samples_valid = false;
    }
    window_samples_valid
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, io::Error> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("missing {field}")))
}

fn record_device_identity(input_dir: &Path, run_id: &str, identity: &Value) -> io::Result<()> {
    for field in ["deviceId", "firmwareVersion", "gitSha", "buildId"] {
        if required_string(identity, field)?.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("empty {field}"),
            ));
        }
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(input_dir.join("device-identity.json"))?;
    serde_json::to_writer_pretty(
        &mut file,
        &json!({"runId": run_id, "receivedAtUnixMs": current_unix_millis(), "identity": identity}),
    )
    .map_err(io::Error::other)?;
    file.sync_data()
}

fn report_device_identity(
    input_dir: &Path,
    run_id: &str,
    selector: &str,
) -> io::Result<(Value, Value)> {
    let path = input_dir.join("device-identity.json");
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok((
                json!({"id": selector, "transport": "devd", "identity": null, "availability": "unavailable_device_identity"}),
                json!({"version": null, "identity": null, "availability": "unavailable_device_identity"}),
            ));
        }
        Err(error) => return Err(error),
    };
    let captured: Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if captured.get("runId").and_then(Value::as_str) != Some(run_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "device identity belongs to another run",
        ));
    }
    let identity = captured.get("identity").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "recorded device identity missing",
        )
    })?;
    Ok((
        json!({"id": required_string(identity, "deviceId")?, "selector": selector, "transport": "devd", "identity": identity, "availability": "device_local"}),
        json!({"version": required_string(identity, "firmwareVersion")?, "gitSha": required_string(identity, "gitSha")?, "buildId": required_string(identity, "buildId")?, "source": "device_identity", "availability": "device_local"}),
    ))
}

#[cfg(test)]
fn render_unsupported_pps3a(
    device_id: &str,
    output_root: &PathBuf,
    bundle_dir: Option<PathBuf>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    render_host_failure(HostFailureInput {
        device_id,
        output_root,
        bundle_dir,
        run_id: None,
        power_class: "pps3a",
        reason: "pps3a_not_supported",
        no_device_command: true,
        attempted_candidate: None,
        selected_targets: None,
    })
}

struct InterruptedRunInput<'a> {
    device_id: &'a str,
    output_root: &'a PathBuf,
    bundle_dir: Option<PathBuf>,
    run_id: &'a str,
    candidate_id: &'a str,
    seed_candidate_hash: &'a str,
    candidate_profile: &'a Value,
    statuses: &'a [Value],
    selected_targets: &'a [i16],
    execution_order: &'a [i16],
    reason: &'a str,
}

fn render_interrupted_run(
    input: InterruptedRunInput<'_>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let candidate_hash = input
        .statuses
        .last()
        .and_then(|status| status.get("candidateHash"))
        .and_then(Value::as_str)
        .unwrap_or(input.seed_candidate_hash);
    let input_dir = input.output_root.join(input.run_id);
    fs::create_dir_all(&input_dir)?;
    write_v2_input(V2Input {
        input_dir: &input_dir,
        device_id: input.device_id,
        run_id: input.run_id,
        candidate_id: input.candidate_id,
        candidate_hash,
        candidate_profile: input.candidate_profile,
        statuses: input.statuses,
        host_interruption: Some(input.reason),
        selected_targets: input.selected_targets,
        execution_order: input.execution_order,
    })?;
    let mut rendered = thermal_tuning_v2_report::render_v2_bundle(ThermalTuningV2ReportInput {
        input_dir: input_dir.clone(),
        output_dir: input.bundle_dir,
    })?;
    rendered["rawInputDir"] = json!(input_dir);
    Ok(rendered)
}

struct HostFailureInput<'a> {
    device_id: &'a str,
    output_root: &'a Path,
    bundle_dir: Option<PathBuf>,
    run_id: Option<&'a str>,
    power_class: &'a str,
    reason: &'a str,
    no_device_command: bool,
    attempted_candidate: Option<(&'a str, &'a str, &'a Value)>,
    selected_targets: Option<&'a [i16]>,
}

fn render_host_failure(
    input: HostFailureInput<'_>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let HostFailureInput {
        device_id,
        output_root,
        bundle_dir,
        run_id,
        power_class,
        reason,
        no_device_command,
        attempted_candidate,
        selected_targets,
    } = input;
    let run_id = run_id
        .map(str::to_owned)
        .unwrap_or_else(|| format!("firmware-failed-{}", current_unix_millis()));
    let input_dir = output_root.join(&run_id);
    fs::create_dir_all(&input_dir)?;
    let (mut device, firmware) = report_device_identity(&input_dir, &run_id, device_id)?;
    device["transport"] = json!(if no_device_command {
        "host-preflight-no-device-command"
    } else {
        "device-command-unconfirmed"
    });
    let bundle = json!({
        "schema": "thermal-tuning-v2",
        "runId": run_id,
        "engine": "firmware",
        "device": device,
        "firmware": firmware,
        "powerClass": power_class,
        "sourceClass": "unavailable",
        "ppsControlPolicyVersion": null,
        "ppsVoltageBounds": {"absoluteMinMv": null, "absoluteMaxMv": null, "policyFloorMv": null, "apdoMinMv": null, "apdoMaxMv": null, "availability": "device_not_queried"},
        "targetSet": {"physicalC": parse_thermal_targets(Some(FIRMWARE_PHYSICAL_TARGETS_C))?, "selectedC": selected_targets.unwrap_or(&[]), "executionOrderC": selected_targets.map(selected_execution_order).unwrap_or_default()},
        "qualificationScope": {"kind": "host-failure", "targetCount": selected_targets.map_or(0, <[i16]>::len), "diagnosticOnly": true},
        "terminalDisposition": "failed",
        "reviewDisposition": "review_incomplete",
        "traceOrigin": if no_device_command { "host_preflight" } else { "host_unconfirmed" },
        "rejectionReason": reason,
        "reportRuns": [],
        "candidate": {"candidateId": attempted_candidate.map(|candidate| candidate.0), "candidateHash": attempted_candidate.map(|candidate| candidate.1), "powerClass": power_class, "promotionState": "unavailable"},
        "referenceComparison": {"status": "not_applicable_host_failure"},
        "capability": {"localEvidence": {"vin": "unavailable", "ppsContract": "unavailable", "physicalPower": "unavailable"}}
    });
    let candidate = json!({
        "schema": "thermal-tuning-v2",
        "runId": run_id,
        "candidateId": attempted_candidate.map(|candidate| candidate.0),
        "candidateHash": attempted_candidate.map(|candidate| candidate.1),
        "powerClass": power_class,
        "profile": attempted_candidate.map(|candidate| candidate.2),
        "promotion": {"state": "unavailable", "reason": reason}
    });
    fs::write(
        input_dir.join("run.bundle.json"),
        serde_json::to_vec_pretty(&bundle)?,
    )?;
    fs::write(
        input_dir.join("thermal-profile.candidate.json"),
        serde_json::to_vec_pretty(&candidate)?,
    )?;
    write_ndjson(&input_dir.join("samples.ndjson"), &[])?;
    write_ndjson(
        &input_dir.join("decision-ledger.ndjson"),
        &[json!({
            "sequence": 0,
            "elapsedMs": 0,
            "kind": "safety",
            "phase": "failed",
            "reason": reason,
            "origin": if no_device_command { "host_preflight" } else { "host_unconfirmed" },
            "heaterState": if no_device_command { "not_commanded" } else { "unconfirmed" }
        })],
    )?;
    thermal_tuning_v2_report::render_v2_bundle(ThermalTuningV2ReportInput {
        input_dir,
        output_dir: bundle_dir,
    })
}

async fn confirm_terminal_tuning_events(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    run_id: &str,
    sequence: u64,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let confirmed =
        request_firmware_event_read(client, resolved, lease_id, run_id, Some(sequence)).await?;
    if confirmed.pointer("/trace/gap").and_then(Value::as_bool) != Some(false)
        || confirmed
            .pointer("/trace/lastSequence")
            .and_then(Value::as_u64)
            != Some(sequence)
    {
        return Err("terminal tuning events were not confirmed".into());
    }
    Ok(confirmed)
}

fn next_event_sequence(status: &Value) -> Option<u64> {
    status
        .pointer("/events/nextSequence")
        .and_then(Value::as_u64)
}

fn is_confirmed_rejected_start(
    power_class: &str,
    status: &Value,
    run_id: &str,
    candidate_id: &str,
    candidate_hash: &str,
) -> bool {
    status.get("runId").and_then(Value::as_str) == Some(run_id)
        && status.get("candidateId").and_then(Value::as_str) == Some(candidate_id)
        && status.get("candidateHash").and_then(Value::as_str) == Some(candidate_hash)
        && status.get("powerClass").and_then(Value::as_str) == Some(power_class)
        && status.get("terminalDisposition").and_then(Value::as_str) == Some("failed")
        && status.get("armed").and_then(Value::as_bool) == Some(false)
}

fn firmware_read_command(run_id: &str, after_sequence: Option<u64>) -> Value {
    let mut command = json!({"op": "read", "runId": run_id});
    if let Some(sequence) = after_sequence {
        command["afterSequence"] = json!(sequence);
    }
    command
}

async fn request_firmware_event_read(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    run_id: &str,
    after_sequence: Option<u64>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let status = request_leased(
        client,
        resolved,
        lease_id,
        Method::POST,
        "/thermal-tuning",
        Some(firmware_read_command(run_id, after_sequence)),
    )
    .await?;
    if status.get("runId").and_then(Value::as_str) != Some(run_id) {
        return Err("tuning_run_lost: device no longer reports the requested tuning run".into());
    }
    Ok(status)
}

fn firmware_start_command(
    power_class: &str,
    run_id: &str,
    candidate_id: &str,
    candidate_hash: &str,
    candidate_profile: &Value,
    execution_order: &[i16],
) -> Value {
    json!({
        "op": "start",
        "runId": run_id,
        "candidateId": candidate_id,
        "candidateHash": candidate_hash,
        "powerClass": power_class,
        "targetsC": execution_order,
        "candidateProfile": {
            "op": "preview",
            "bank": power_class,
            "profile": candidate_profile
        }
    })
}

fn event_stream_caught_up(status: &Value, after_sequence: Option<u64>) -> bool {
    if status_trace_gap(status)
        && status
            .get("terminalDisposition")
            .and_then(Value::as_str)
            .is_some_and(|value| matches!(value, "failed" | "canceled"))
        && status
            .pointer("/events/events")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    {
        return true;
    }
    match status
        .pointer("/trace/lastSequence")
        .and_then(Value::as_u64)
    {
        Some(last_sequence) => after_sequence.is_some_and(|received| received >= last_sequence),
        None => true,
    }
}

fn status_trace_gap(status: &Value) -> bool {
    status.pointer("/trace/gap").and_then(Value::as_bool) == Some(true)
}

fn firmware_candidate_profile_value(profile: &Value) -> Value {
    let mut profile = profile.clone();
    let Some(object) = profile.as_object_mut() else {
        return profile;
    };
    let mut settings = object
        .remove("settings")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let first_point = object
        .get("points")
        .and_then(Value::as_array)
        .and_then(|points| points.iter().find(|point| point.is_object()))
        .and_then(Value::as_object);
    let inherited = [
        "warmupReenterCentiC",
        "holdEntryCentiC",
        "holdExitCentiC",
        "holdOnCentiC",
        "holdOffCentiC",
        "overshootCutoffCentiC",
        "holdKpPermillePerC",
        "holdKiPermillePerCTick",
        "holdBlendTicks",
        "holdReheatPowerPermille",
        "approachLeadTicks",
        "holdLeadTicks",
    ];
    for key in inherited {
        if !settings.contains_key(key)
            && let Some(value) = first_point.and_then(|point| point.get(key))
        {
            settings.insert(key.to_string(), value.clone());
        }
    }
    let defaults = [
        ("tempFilterAlphaPermille", json!(750)),
        ("warmupReenterCentiC", json!(400)),
        ("holdEntryCentiC", json!(90)),
        ("holdExitCentiC", json!(200)),
        ("holdOnCentiC", json!(30)),
        ("holdOffCentiC", json!(5)),
        ("overshootCutoffCentiC", json!(25)),
        ("approachMaxTicks", json!(5)),
        ("approachMinPowerRatioPermille", json!(0)),
        ("holdKpPermillePerC", json!(120)),
        ("holdKiPermillePerCTick", json!(12)),
        ("holdBlendTicks", json!(12)),
        ("holdReheatPowerPermille", json!(0)),
        ("approachLeadTicks", json!(0)),
        ("holdLeadTicks", json!(0)),
        ("autoAdjustableWorkingFloorMv", json!(5000)),
        ("heaterCurrentReserveMa", json!(200)),
    ];
    for (key, value) in defaults {
        settings.entry(key.to_string()).or_insert(value);
    }
    object.insert("settings".to_string(), Value::Object(settings));
    if let Some(points) = object.get_mut("points").and_then(Value::as_array_mut) {
        points.resize(
            flux_purr_firmware::frontpanel::FRONTPANEL_PRESET_COUNT,
            Value::Null,
        );
    }
    Value::Object(std::mem::take(object))
}

fn parse_firmware_seed_profile(
    imported: Value,
    power_class: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    if imported.get("schema").and_then(Value::as_str) == Some("thermal-tuning-v2")
        && imported.get("powerClass").and_then(Value::as_str) != Some(power_class)
    {
        return Err("firmware seed export requires the requested power class".into());
    }
    let is_export = imported.get("schema").and_then(Value::as_str) == Some("thermal-tuning-v2");
    let declared_hash = imported.get("candidateHash").cloned();
    if is_export && imported.pointer("/profile/bank").and_then(Value::as_str) != Some(power_class) {
        return Err("firmware seed export requires the requested profile bank".into());
    }
    let mut profile = super::thermal_profile_package_from_value(imported);
    let points = profile.get("points").and_then(Value::as_array);
    if !profile.get("settings").is_some_and(Value::is_object)
        || !points.is_some_and(|points| {
            points
                .iter()
                .any(|point| point.get("targetTempC").is_some_and(Value::is_number))
        })
    {
        return Err("firmware seed requires explicit settings and target points".into());
    }
    if is_export {
        // Exported profiles are complete wire values, not legacy inherited settings.
        let points = profile["points"].as_array_mut().unwrap();
        let capacity = flux_purr_firmware::frontpanel::FRONTPANEL_PRESET_COUNT;
        if points.len() > capacity {
            return Err("firmware seed export has too many target points".into());
        }
        points.resize(capacity, Value::Null);
        let wire: ThermalControlProfileWire = serde_json::from_value(profile)?;
        let targets: Vec<_> = wire
            .points
            .iter()
            .flatten()
            .map(|point| point.target_temp_c)
            .collect();
        if targets != THERMAL_TUNING_TARGETS_C {
            return Err("firmware seed export requires the complete ordered target set".into());
        }
        let mut config: ThermalControlProfileConfig = wire.into();
        config.sanitize();
        if ThermalControlProfileWire::from(config) != wire {
            return Err("firmware seed export contains noncanonical profile values".into());
        }
        let value = serde_json::to_value(wire)?;
        if declared_hash.is_some_and(|hash| {
            hash.as_str()
                .is_none_or(|hash| !profile_matches_hash(&value, hash))
        }) {
            return Err("firmware seed export candidate hash does not match its profile".into());
        }
        return Ok(value);
    }
    materialize_firmware_seed_profile(thermal_candidate_profile_from_value(profile))
}

fn materialize_firmware_seed_profile(
    seed: ThermalCandidateProfile,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut points = Vec::with_capacity(THERMAL_TUNING_TARGETS_C.len());
    for target in THERMAL_TUNING_TARGETS_C {
        let point = thermal_candidate_point(&seed, target)
            .or_else(|| thermal_interpolated_candidate_point(&seed, target))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("5A seed cannot materialize {target}C target"),
                )
            })?;
        points.push(point);
    }
    Ok(firmware_candidate_profile_value(
        &thermal_candidate_profile_to_value(&ThermalCandidateProfile {
            settings: seed.settings,
            points,
        }),
    ))
}

fn calibrated_firmware_seed_profile(
    runtime: &Value,
    calibration: &Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    use flux_purr_firmware::memory::heater_resistance_ohms_from_curve;
    use flux_purr_firmware::thermal_plant::{
        natural_cooling_loss_mw, natural_cooling_rate_c_per_s,
    };

    let active = calibration
        .get("activeResult")
        .ok_or("active calibration required for a cold seed")?;
    let (model, response) = calibrated_seed_model(runtime, active)?;
    // Ambient belongs to this fit. The current (possibly hot) plate reading is
    // not an ambient observation and must never substitute for a missing one.
    let ambient = calibration
        .pointer("/tracePage/points")
        .and_then(Value::as_array)
        .and_then(|points| {
            points.iter().find(|point| {
                point.get("sampleIndex").and_then(Value::as_u64) == Some(0)
                    && point.get("phase").and_then(Value::as_str) == Some("ambient")
            })
        })
        .and_then(|point| point.get("temperatureCentiC"))
        .and_then(Value::as_i64)
        .filter(|value| (-5_000..=15_000).contains(value))
        .map(|value| value as f32 / 100.0)
        .ok_or("calibration ambient sample required")?;
    if natural_cooling_rate_c_per_s(model, 60.0, ambient).is_none() {
        return Err("invalid calibration cannot initialize thermal tuning".into());
    }
    let voltage_mv = runtime
        .get("ppsCapabilityMaxMv")
        .and_then(Value::as_u64)
        .filter(|value| (5_000..=28_000).contains(value))
        .ok_or("valid PPS voltage capability required")? as f32;
    let current_ma = runtime
        .get("ppsCapabilityMaxMa")
        .and_then(Value::as_u64)
        .filter(|value| (1_000..=5_000).contains(value))
        .ok_or("valid PPS current capability required")? as f32;
    let curve = calibrated_seed_curve(active)?;
    let mut seeded = Vec::new();
    for target in THERMAL_TUNING_TARGETS_C {
        let temperature_c = f32::from(target);
        let resistance = heater_resistance_ohms_from_curve(&curve, temperature_c)
            .ok_or("calibrated target resistance unavailable")?
            .max(3.2 * (1.0 + 0.00393 * (temperature_c - 20.0)));
        // Use the same resistance and contract bounds as production Pmax,
        // rather than treating the nominal bank name as measured input power.
        let maximum_power_mw = (voltage_mv * voltage_mv / resistance / 1_000.0)
            .min(voltage_mv * current_ma / 1_000.0)
            .max(1.0);
        let loss_mw = natural_cooling_loss_mw(model, temperature_c, ambient);
        let delay_s = (response.delay_ms + response.decay_ms) as f32 / 1_000.0;
        let brake_centi_c = flux_purr_firmware::thermal_response::reference_brake_c(
            response,
            model,
            ambient,
            maximum_power_mw,
            temperature_c,
        ) * 100.0;
        let hold_permille = (loss_mw / maximum_power_mw * 1_000.0)
            .round()
            .clamp(50.0, 1_000.0) as u16;
        // An integrating heat plant with transport delay: choose a closed-loop
        // response of 3 delays (IMC lambda), so Kp = C / (Pmax * 4 delays).
        let kp = (model.thermal_capacity_mj_per_c / (maximum_power_mw * 4.0 * delay_s.max(0.4))
            * 1_000.0)
            .round()
            .clamp(1.0, 1_000.0) as u16;
        seeded.push(json!({
            "targetTempC": target,
            "brakeDistanceCentiC": brake_centi_c.round().clamp(100.0, 5_000.0) as u16,
            "warmupPowerPermille": 1_000, "warmupReenterCentiC": 400,
            "approachPowerPermille": 1_000, "approachFloorPowerPermille": 50,
            "approachDampingExponentPermille": 1_000, "approachTailWindowCentiC": 0,
            "approachLeadTicks": 0, "holdLeadTicks": 0,
            "holdPowerPermille": hold_permille, "holdReheatPowerPermille": hold_permille,
            "holdEntryCentiC": 50, "holdExitCentiC": 150,
            "holdOnCentiC": 30, "holdOffCentiC": 120, "overshootCutoffCentiC": 150,
            "holdKpPermillePerC": kp, "holdKiPermillePerCTick": 1, "holdBlendTicks": 1
        }));
    }
    Ok(firmware_candidate_profile_value(&json!({
        "settings": {"tempFilterAlphaPermille": 750, "approachMaxTicks": 5,
            "approachMinPowerRatioPermille": 0, "autoAdjustableWorkingFloorMv": 5_000,
            "heaterCurrentReserveMa": 200},
        "points": seeded
    })))
}

fn calibrated_seed_curve(
    active: &Value,
) -> Result<flux_purr_firmware::memory::HeaterCurveConfig, Box<dyn std::error::Error + Send + Sync>>
{
    use flux_purr_firmware::memory::{HeaterCurveConfig, HeaterCurvePoint};
    let mut curve = HeaterCurveConfig::default();
    let points = active
        .pointer("/curve/points")
        .and_then(Value::as_array)
        .ok_or("calibrated resistance curve required")?;
    if points.len() > curve.points.len() {
        return Err("calibrated resistance curve exceeds its point capacity".into());
    }
    for (index, point) in points
        .iter()
        .enumerate()
        .filter(|(_, point)| !point.is_null())
    {
        curve.points[index] = Some(HeaterCurvePoint {
            temp_centi_c: point
                .get("tempCentiC")
                .and_then(Value::as_i64)
                .and_then(|value| i16::try_from(value).ok())
                .ok_or("calibrated curve temperature required")?,
            resistance_milliohms: point
                .get("resistanceMilliohms")
                .and_then(Value::as_u64)
                .and_then(|value| u16::try_from(value).ok())
                .filter(|value| *value > 0)
                .ok_or("positive calibrated resistance required")?,
        });
    }
    if curve.points.iter().flatten().count() < 2 {
        return Err(
            "cold seed requires a calibrated resistance curve with at least two points".into(),
        );
    }
    Ok(curve)
}

fn calibrated_seed_model(
    runtime: &Value,
    active: &Value,
) -> Result<
    (
        flux_purr_firmware::memory::ThermalPlantProjection,
        flux_purr_firmware::thermal_response::HeatResponse,
    ),
    Box<dyn std::error::Error + Send + Sync>,
> {
    use flux_purr_firmware::memory::ThermalPlantProjection;
    let transaction = active
        .get("transactionId")
        .and_then(Value::as_u64)
        .filter(|id| *id != 0)
        .ok_or("active calibration identity required")?;
    if runtime
        .pointer("/thermalPlantModel/projectionValid")
        .and_then(Value::as_bool)
        != Some(true)
        || runtime
            .pointer("/thermalPlantModel/activeTransactionId")
            .and_then(Value::as_u64)
            != Some(transaction)
    {
        return Err("cold seed requires the matching active thermal calibration".into());
    }
    let number = |key: &str| -> Result<f32, Box<dyn std::error::Error + Send + Sync>> {
        active
            .get(key)
            .and_then(Value::as_f64)
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("calibration {key} required").into())
    };
    let model = ThermalPlantProjection {
        convection_mw_per_c: number("convectionMwPerC")?,
        radiation_mw_per_k4: number("radiationMwPerK4")?,
        thermal_capacity_mj_per_c: number("thermalCapacityMjPerC")?,
        transport_delay_ms: active
            .get("transportDelayMs")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or("calibration delay required")?,
    };
    let response: flux_purr_firmware::thermal_response::HeatResponse = serde_json::from_value(
        active
            .get("heatResponse")
            .cloned()
            .ok_or("calibrated shutdown response required")?,
    )?;
    if response.decay_ms < 100
        || response.decay_ms > 12_000
        || response.delay_ms > 5_000
        || !response.rmse_c.is_finite()
        || response.rmse_c > 1.0
        || !response.maximum_error_c.is_finite()
        || response.maximum_error_c > 2.0
    {
        return Err("invalid calibrated shutdown response".into());
    }
    Ok((model, response))
}

fn firmware_candidate_profile_hash(
    profile: &Value,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut wire_value = profile.clone();
    let points = wire_value
        .get_mut("points")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "candidate points required"))?;
    let capacity = flux_purr_firmware::frontpanel::FRONTPANEL_PRESET_COUNT;
    if points.len() > capacity {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "too many candidate points").into(),
        );
    }
    points.resize(capacity, Value::Null);
    let wire: ThermalControlProfileWire = serde_json::from_value(wire_value)?;
    let mut config: ThermalControlProfileConfig = wire.into();
    config.sanitize();
    let hash = hash_profile(&config)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "candidate hash failed"))?;
    Ok(hash.to_string())
}

fn validate_firmware_args(
    args: &ThermalFlagshipTuneArgs,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    if args.engine != ThermalTuneEngine::Firmware {
        return Err("firmware runner received a non-firmware engine".into());
    }
    if !matches!(
        args.profile_mode,
        ThermalProfileMode::W100 | ThermalProfileMode::W65
    ) {
        return Err("firmware thermal tuning requires an explicit 65w or 100w profile mode".into());
    }
    if !args.source_id.is_empty()
        || !args.source_url.is_empty()
        || args.source_voltage_v.is_some()
        || args.source_current_a.is_some()
        || args.source_power_watts.is_some()
    {
        return Err(
            "--engine firmware rejects external source-id/source-url/source overrides".into(),
        );
    }
    let requested = parse_thermal_targets(Some(&args.tune_targets_c))?;
    let expected = parse_thermal_targets(Some(FIRMWARE_PHYSICAL_TARGETS_C))?;
    if requested.iter().any(|target| !expected.contains(target)) {
        return Err("firmware engine targets must belong to the eight-target tuning set".into());
    }
    Ok(requested)
}

fn selected_execution_order(selected_targets: &[i16]) -> Vec<i16> {
    THERMAL_TUNING_EXECUTION_ORDER_C
        .into_iter()
        .filter(|target| selected_targets.contains(target))
        .collect()
}

fn firmware_host_watchdog(target_count: usize) -> Duration {
    Duration::from_millis(
        THERMAL_TUNING_TARGET_BUDGET_MS
            .saturating_mul(target_count as u64)
            .saturating_add(60_000),
    )
}

struct V2Input<'a> {
    input_dir: &'a Path,
    device_id: &'a str,
    run_id: &'a str,
    candidate_id: &'a str,
    candidate_hash: &'a str,
    candidate_profile: &'a Value,
    statuses: &'a [Value],
    host_interruption: Option<&'a str>,
    selected_targets: &'a [i16],
    execution_order: &'a [i16],
}

pub(crate) fn prepare_v2_device_journal(
    input_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if input_dir.join("run.bundle.json").exists() {
        return Ok(());
    }
    let statuses: Vec<Value> = fs::read_to_string(input_dir.join("device-statuses.ndjson"))?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let first = statuses.first().ok_or("empty firmware device journal")?;
    let run_id = required_string(first, "runId")?;
    let candidate_id = required_string(first, "candidateId")?;
    let mut selected_targets: Vec<i16> = serde_json::from_value(first["targetsC"].clone())?;
    let execution_order = selected_targets.clone();
    selected_targets.sort_unstable();
    if selected_targets.is_empty()
        || selected_targets.windows(2).any(|pair| pair[0] == pair[1])
        || selected_targets
            .iter()
            .any(|target| !THERMAL_TUNING_TARGETS_C.contains(target))
        || selected_execution_order(&selected_targets) != execution_order
    {
        return Err("journal target scope is not the frozen execution scope".into());
    }
    validate_device_journal_identity(&statuses, first)?;
    let last = statuses.last().ok_or("empty firmware device journal")?;
    let candidate_profile = first
        .get("candidateProfile")
        .ok_or("journal seed profile missing")?;
    let candidate_hash = required_string(last, "candidateHash")?;
    let interrupted = !matches!(
        last["terminalDisposition"].as_str(),
        Some("failed" | "canceled" | "candidate_ready" | "passed")
    );
    write_v2_input(V2Input {
        input_dir,
        device_id: "unavailable_device_selector",
        run_id,
        candidate_id,
        candidate_hash,
        candidate_profile,
        statuses: &statuses,
        host_interruption: interrupted.then_some("host_process_stopped"),
        selected_targets: &selected_targets,
        execution_order: &execution_order,
    })
}

fn validate_device_journal_identity(
    statuses: &[Value],
    first: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let power_class = required_power_class(first)?;
    for status in statuses {
        if status["runId"] != first["runId"]
            || status["candidateId"] != first["candidateId"]
            || status["targetsC"] != first["targetsC"]
            || status["powerClass"] != power_class
        {
            return Err("device journal mixes run identity, power class or target scope".into());
        }
        if let Some(profile) = status
            .get("candidateProfile")
            .filter(|value| !value.is_null())
            && firmware_candidate_profile_hash(profile)?
                != required_string(status, "candidateHash")?
        {
            return Err("device journal candidate profile hash does not match".into());
        }
    }
    Ok(())
}

fn write_v2_input(input: V2Input<'_>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let V2Input {
        input_dir,
        device_id,
        run_id,
        candidate_id,
        candidate_hash,
        candidate_profile: _,
        statuses,
        host_interruption,
        selected_targets,
        execution_order,
    } = input;
    let first = statuses.first().ok_or("empty firmware device journal")?;
    let power_class = required_power_class(first)?;
    validate_device_journal_identity(statuses, first)?;
    let (device, firmware) = report_device_identity(input_dir, run_id, device_id)?;
    let samples = collect_events(statuses, true);
    let ledger = collect_events(statuses, false);
    let (device_trace, device_trace_verification) = verify_device_trace(statuses);
    let device_terminal = statuses
        .last()
        .and_then(|status| status.get("terminalDisposition"))
        .and_then(Value::as_str)
        .unwrap_or("failed");
    let terminal = if host_interruption.is_some() {
        "failed"
    } else {
        device_terminal
    };
    let targets = parse_thermal_targets(Some(FIRMWARE_PHYSICAL_TARGETS_C))?;
    let report_runs = build_report_runs(
        selected_targets,
        &samples,
        &ledger,
        candidate_id,
        candidate_hash,
    );
    let (candidate, candidate_metadata) = report_candidate_metadata(&input, power_class, terminal);
    let (pps_policy_version, pps_voltage_bounds) = report_pps_metadata(statuses);
    let bundle = json!({
        "schema": "thermal-tuning-v2",
        "runId": run_id,
        "engine": "firmware",
        "device": device,
        "firmware": firmware,
        "powerClass": power_class,
        "sourceClass": "device-local",
        "ppsControlPolicyVersion": pps_policy_version,
        "ppsVoltageBounds": pps_voltage_bounds,
        "targetSet": {"physicalC": targets, "selectedC": selected_targets, "executionOrderC": execution_order},
        "qualificationScope": {"kind": if selected_targets.len() == targets.len() { format!("default-{power_class}-full-batch") } else { "diagnostic-retest".to_string() }, "targetCount": selected_targets.len(), "diagnosticOnly": selected_targets.len() != targets.len()},
        "terminalDisposition": terminal,
        "reviewDisposition": if host_interruption.is_some() { json!("review_incomplete") } else { statuses.last().and_then(|status| status.get("reviewDisposition")).cloned().unwrap_or_else(|| json!("review_incomplete")) },
        "hostInterruption": host_interruption.map(|reason| json!({"reason": reason, "lastDeviceTerminalDisposition": device_terminal, "deviceState": "unconfirmed"})),
        "deviceTrace": device_trace,
        "deviceTraceVerification": device_trace_verification,
        "reportRuns": report_runs,
        "candidate": candidate_metadata,
        "referenceComparison": {
            "fixture": "external-5a-conformance-v1",
            "status": "pending_device_conformance_evidence",
            "algorithmCopied": false
        },
        "capability": {"localEvidence": {
            "vin": "available",
            "ppsContract": "available",
            "externalVbus": "unavailable",
            "physicalPower": "unavailable"
        }}
    });

    fs::write(
        input_dir.join("run.bundle.json"),
        serde_json::to_vec_pretty(&bundle)?,
    )?;
    fs::write(
        input_dir.join("thermal-profile.candidate.json"),
        serde_json::to_vec_pretty(&candidate)?,
    )?;
    write_ndjson(&input_dir.join("samples.ndjson"), &samples)?;
    write_ndjson(&input_dir.join("decision-ledger.ndjson"), &ledger)?;
    fs::write(
        input_dir.join("device-statuses.json"),
        serde_json::to_vec(&statuses)?,
    )?;
    Ok(())
}

fn report_pps_metadata(statuses: &[Value]) -> (Option<&Value>, Value) {
    let pps_bounds = statuses.iter().rev().find_map(|status| {
        status
            .get("ppsVoltageBounds")
            .filter(|value| !value.is_null())
    });
    let pps_policy_version = statuses.iter().rev().find_map(|status| {
        status
            .get("ppsControlPolicyVersion")
            .filter(|value| !value.is_null())
    });
    let bound = |field: &str| {
        pps_bounds
            .and_then(|value| value.get(field))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let pps_voltage_bounds = json!({
        "absoluteMinMv": bound("absoluteMinMv"),
        "absoluteMaxMv": bound("absoluteMaxMv"),
        "policyFloorMv": bound("policyFloorMv"),
        "apdoMinMv": bound("apdoMinMv"),
        "apdoMaxMv": bound("apdoMaxMv"),
        "availability": if pps_bounds.is_none() {
            "unavailable_device_status_field"
        } else if pps_bounds.and_then(|value| value.get("apdoMinMv")).and_then(Value::as_u64).is_some()
            && pps_bounds.and_then(|value| value.get("apdoMaxMv")).and_then(Value::as_u64).is_some()
        {
            "device_local"
        } else {
            "unavailable_selected_apdo"
        }
    });
    (pps_policy_version, pps_voltage_bounds)
}

fn report_candidate_metadata(
    input: &V2Input<'_>,
    power_class: &str,
    terminal: &str,
) -> (Value, Value) {
    let V2Input {
        run_id,
        candidate_id,
        candidate_hash,
        candidate_profile,
        statuses,
        ..
    } = *input;
    let effective_candidate_profile = statuses
        .iter()
        .rev()
        .filter(|status| {
            status.get("candidateHash").and_then(Value::as_str) == Some(candidate_hash)
        })
        .find_map(|status| {
            status
                .get("candidateProfile")
                .filter(|value| profile_matches_hash(value, candidate_hash))
        })
        .or_else(|| {
            profile_matches_hash(candidate_profile, candidate_hash).then_some(candidate_profile)
        });
    let profile_source = if effective_candidate_profile.is_some() {
        "device_reported"
    } else {
        "unavailable_matching_candidate_profile"
    };
    let save_receipt = statuses
        .iter()
        .rev()
        .find_map(|status| status.get("saveReceipt").filter(|value| !value.is_null()))
        .cloned();
    let preview_receipt = statuses
        .iter()
        .rev()
        .find_map(|status| {
            status
                .get("previewReceipt")
                .filter(|value| !value.is_null())
        })
        .cloned();
    let candidate_state = if effective_candidate_profile.is_none() {
        "rejected"
    } else if verified_saved_receipt(
        power_class,
        run_id,
        candidate_id,
        candidate_hash,
        preview_receipt.as_ref(),
        save_receipt.as_ref(),
    ) {
        "saved"
    } else if terminal == "candidate_ready" {
        if statuses
            .last()
            .and_then(|status| status.get("sealed"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            "sealed"
        } else {
            "candidate_ready"
        }
    } else {
        "rejected"
    };
    let candidate = json!({
        "schema": "thermal-tuning-v2",
        "runId": run_id,
        "candidateId": candidate_id,
        "candidateHash": candidate_hash,
        "powerClass": power_class,
        "promotion": {
            "state": candidate_state,
            "previewReceipt": preview_receipt,
            "saveReceipt": save_receipt
        },
        "profile": {"bank": power_class, "source": profile_source, "value": effective_candidate_profile}
    });
    (
        candidate,
        json!({"candidateId": candidate_id, "candidateHash": candidate_hash,
        "powerClass": power_class, "profileAvailability": profile_source, "promotionState": candidate_state}),
    )
}

fn profile_matches_hash(profile: &Value, candidate_hash: &str) -> bool {
    firmware_candidate_profile_hash(profile).ok().as_deref() == Some(candidate_hash)
}

fn verified_saved_receipt(
    power_class: &str,
    run_id: &str,
    candidate_id: &str,
    candidate_hash: &str,
    preview: Option<&Value>,
    save: Option<&Value>,
) -> bool {
    let Some((preview, save)) = preview.zip(save) else {
        return false;
    };
    [preview, save].into_iter().all(|receipt| {
        receipt.get("runId").and_then(Value::as_str) == Some(run_id)
            && receipt.get("candidateId").and_then(Value::as_str) == Some(candidate_id)
            && receipt.get("candidateHash").and_then(Value::as_str) == Some(candidate_hash)
            && receipt.get("powerClass").and_then(Value::as_str) == Some(power_class)
    }) && preview.get("ramAppliedHash").and_then(Value::as_str) == Some(candidate_hash)
        && save.get("bank").and_then(Value::as_str) == Some(power_class)
        && save.get("bankHash").and_then(Value::as_str) == Some(candidate_hash)
        && save.get("atomic").and_then(Value::as_bool) == Some(true)
        && save.get("readbackVerified").and_then(Value::as_bool) == Some(true)
        && save.get("bankRevision").and_then(Value::as_u64).is_some()
}

fn verify_device_trace(statuses: &[Value]) -> (Value, Value) {
    let trace = statuses
        .last()
        .and_then(|status| status.get("trace"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut reasons = Vec::<String>::new();
    let mut by_sequence = BTreeMap::<u64, Value>::new();
    for status in statuses {
        let Some(events) = status.pointer("/events/events").and_then(Value::as_array) else {
            continue;
        };
        for event in events {
            let Some(sequence) = event.get("sequence").and_then(Value::as_u64) else {
                reasons.push("device event missing sequence".to_string());
                continue;
            };
            if let Some(previous) = by_sequence.insert(sequence, event.clone())
                && previous != *event
            {
                reasons.push(format!("device event {sequence} changed across pages"));
            }
        }
    }
    let first = by_sequence.first_key_value().map(|(sequence, _)| *sequence);
    let last = by_sequence.last_key_value().map(|(sequence, _)| *sequence);
    if first != trace.get("firstSequence").and_then(Value::as_u64)
        || last != trace.get("lastSequence").and_then(Value::as_u64)
    {
        reasons.push("device trace sequence range does not match collected events".to_string());
    }
    if let (Some(first), Some(last)) = (first, last)
        && by_sequence.len() != (last - first + 1) as usize
    {
        reasons.push("device trace has a missing sequence".to_string());
    }
    if trace.get("complete").and_then(Value::as_bool) != Some(true)
        || trace.get("gap").and_then(Value::as_bool) != Some(false)
    {
        reasons.push("device did not declare a complete gap-free trace".to_string());
    }
    let mut typed = Vec::<ThermalTuningEvent>::new();
    for (sequence, event) in by_sequence {
        match serde_json::from_value(event) {
            Ok(event) => typed.push(event),
            Err(error) => reasons.push(format!(
                "device event {sequence} cannot be decoded: {error}"
            )),
        }
    }
    if typed.len() == last.map_or(0, |last| last as usize + 1) {
        let digest = digest_event_chain(&typed);
        if trace.get("digest").and_then(Value::as_str) != Some(digest.as_str()) {
            reasons.push("device trace digest does not match collected events".to_string());
        }
    } else {
        reasons.push(
            "device trace digest cannot be verified against an incomplete event set".to_string(),
        );
    }
    let verified = reasons.is_empty();
    (trace, json!({"verified": verified, "reasons": reasons}))
}

fn build_report_runs(
    targets: &[i16],
    samples: &[Value],
    ledger: &[Value],
    fallback_candidate_id: &str,
    fallback_candidate_hash: &str,
) -> Vec<Value> {
    targets
        .iter()
        .map(|target| {
            let mut trials = trial_events_for_target(ledger, *target)
                .into_iter()
                .map(|event| {
                    build_report_trial(
                        *target,
                        event,
                        samples,
                        ledger,
                        fallback_candidate_id,
                        fallback_candidate_hash,
                    )
                })
                .collect::<Vec<_>>();
            trials.sort_by_key(|trial| {
                trial
                    .get("trialIndex")
                    .and_then(Value::as_i64)
                    .unwrap_or(i64::MAX)
            });
            let adopted = trials.iter().find(|trial| {
                trial.get("disposition").and_then(Value::as_str) == Some("adopted")
                    && trial.get("reviewOutcome").and_then(Value::as_str) == Some("passed")
            });
            let summary_trial = adopted.or_else(|| trials.last());
            let review_outcome = if adopted.is_some() {
                "passed"
            } else {
                "failed"
            };
            let adopted_trial_index = adopted
                .and_then(|trial| trial.get("trialIndex").cloned())
                .unwrap_or(Value::Null);
            let metrics = summary_trial
                .and_then(|trial| trial.get("metrics").cloned())
                .unwrap_or_else(|| json!({}));
            let stop_reason =
                thermal_tuning_v2_report::target_budget_stop_reason(ledger, i64::from(*target))
                    .or_else(|| {
                        summary_trial
                            .and_then(|trial| trial.get("stopReason").and_then(Value::as_str))
                    });
            json!({
                "targetC": target,
                "reviewOutcome": review_outcome,
                "reviewPassed": adopted.is_some(),
                "adoptedTrialIndex": adopted_trial_index,
                "trialCount": trials.len(),
                "stopReason": stop_reason,
                "metrics": metrics,
                "trials": trials
            })
        })
        .collect()
}

fn trial_events_for_target(ledger: &[Value], target: i16) -> Vec<&Value> {
    let mut by_trial = BTreeMap::<u64, &Value>::new();
    for event in ledger {
        if event.get("kind").and_then(Value::as_str) != Some("candidate_trial")
            || event.get("targetC").and_then(Value::as_i64) != Some(i64::from(target))
        {
            continue;
        }
        let Some(index) = event.get("trialIndex").and_then(Value::as_u64) else {
            continue;
        };
        let is_closed = event
            .pointer("/trial/endSequence")
            .and_then(Value::as_u64)
            .is_some();
        if is_closed || !by_trial.contains_key(&index) {
            by_trial.insert(index, event);
        }
    }
    by_trial.into_values().collect()
}

fn trial_hold_evidence(
    target: i16,
    trial_record: &Value,
    decision_record: &Value,
    samples: &[&Value],
) -> (bool, Value, Option<f64>) {
    let independent_confirmation = trial_record.get("purpose").and_then(Value::as_str)
        == Some("hold_confirm")
        && decision_record.get("purpose").and_then(Value::as_str) == Some("hold_confirm");
    let timing = decision_record.get("timing").and_then(|value| {
        serde_json::from_value::<ThermalTuningTimingEvidence>(value.clone()).ok()
    });
    let hold_evidence = verify_hold_samples(timing.as_ref(), target, samples);
    let device_hold_confirm =
        decision_record.get("holdConfirm").and_then(Value::as_bool) == Some(true);
    let seconds =
        (independent_confirmation && device_hold_confirm && hold_evidence["verified"] == true)
            .then(|| hold_evidence["continuousHoldMs"].as_u64().unwrap_or(0) as f64 / 1_000.0);
    (device_hold_confirm, hold_evidence, seconds)
}

fn trial_warmup_request(samples: &[&Value]) -> Option<u64> {
    samples
        .iter()
        .filter(|sample| sample.get("phase").and_then(Value::as_str) == Some("warmup"))
        .filter_map(|sample| {
            sample
                .get("heaterRequestedOutputPermille")
                .and_then(Value::as_u64)
        })
        .max()
}

fn build_report_trial(
    target: i16,
    event: &Value,
    samples: &[Value],
    ledger: &[Value],
    fallback_candidate_id: &str,
    fallback_candidate_hash: &str,
) -> Value {
    let trial_index = event
        .get("trialIndex")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let trial_record = event.get("trial").cloned().unwrap_or_else(|| json!({}));
    let candidate = trial_record.get("candidate").cloned().unwrap_or_else(|| {
        json!({
            "candidateId": fallback_candidate_id,
            "candidateHash": fallback_candidate_hash,
            "powerClass": null
        })
    });
    let decision_record = trial_decision(ledger, target, trial_index);
    let disposition = decision_record
        .get("disposition")
        .and_then(Value::as_str)
        .unwrap_or("rejected");
    let sample_start = trial_record
        .get("sampleStartSequence")
        .and_then(Value::as_u64);
    let sample_end = trial_record
        .get("sampleEndSequence")
        .and_then(Value::as_u64);
    let sample_values = trial_samples(samples, target, trial_index, sample_start, sample_end);
    let bounds = sample_bounds_for_trial(&sample_values);
    let phases = phase_path(&sample_values);
    let warmup_request = trial_warmup_request(&sample_values);
    let settle_time = decision_record
        .get("settleTimeMs")
        .cloned()
        .unwrap_or(Value::Null);
    let limit_ms = decision_record
        .get("hardGateFullSpeedToStableMs")
        .cloned()
        .unwrap_or(Value::Null);
    let (device_hold_confirm, hold_evidence, hold_confirm_seconds) =
        trial_hold_evidence(target, &trial_record, &decision_record, &sample_values);
    let metrics = json!({
        "warmupRequestedOutputPermille": warmup_request,
        "phasePath": phases,
        "maxOvershootC": decision_record.get("maxOvershootCentiC").and_then(Value::as_f64).map(|value| value / 100.0),
        "holdPeakToPeakC": decision_record.get("holdPeakToPeakCentiC").and_then(Value::as_f64).map(|value| value / 100.0),
        "holdConfirmSeconds": hold_confirm_seconds,
        "fullSpeedToStable": {"settleTimeMs": settle_time, "limitMs": limit_ms}
    });
    let mut result = json!({
        "targetC": target,
        "trialIndex": trial_index,
        "trialPurpose": trial_record.get("purpose").cloned().unwrap_or(Value::Null),
        "candidateId": candidate.get("candidateId").cloned().unwrap_or_else(|| json!(fallback_candidate_id)),
        "candidateHash": candidate.get("candidateHash").cloned().unwrap_or_else(|| json!(fallback_candidate_hash)),
        "disposition": disposition,
        "reviewOutcome": "failed",
        "deviceHoldConfirm": device_hold_confirm,
        "holdEvidenceVerification": hold_evidence,
        "metrics": metrics,
        "warmupRequestedOutputPermille": warmup_request,
        "maxOvershootC": decision_record.get("maxOvershootCentiC").and_then(Value::as_f64).map(|value| value / 100.0),
        "holdPeakToPeakC": decision_record.get("holdPeakToPeakCentiC").and_then(Value::as_f64).map(|value| value / 100.0),
        "fullSpeedToStable": {"settleTimeMs": settle_time, "limitMs": limit_ms},
        "cooldownGate": {
            "startTemperatureCentiC": trial_record.get("startTemperatureCentiC").cloned().unwrap_or(Value::Null),
            "thresholdCentiC": trial_record.get("cooldownThresholdCentiC").cloned().unwrap_or(Value::Null)
        },
        "firstSequence": bounds.0,
        "lastSequence": bounds.1,
        "sampleStartSequence": sample_start,
        "sampleEndSequence": sample_end,
        "firstElapsedMs": bounds.2,
        "lastElapsedMs": bounds.3,
        "bounds": {
            "lowerTargetC": candidate.get("lowerTargetC").cloned().unwrap_or(Value::Null),
            "upperTargetC": candidate.get("upperTargetC").cloned().unwrap_or(Value::Null)
        },
        "candidateSeed": {
            "source": candidate.get("seedSource").cloned().unwrap_or_else(|| json!("device_candidate_preview")),
            "lowerTargetC": candidate.get("lowerTargetC").cloned().unwrap_or(Value::Null),
            "upperTargetC": candidate.get("upperTargetC").cloned().unwrap_or(Value::Null)
        },
        "canonicalCandidate": candidate,
        "stopReason": decision_record.get("stopReason").cloned().unwrap_or_else(|| json!(event.get("reason").and_then(Value::as_str).unwrap_or("device_terminal_failure")))
    });
    append_trial_timing(
        &mut result,
        &trial_record,
        &decision_record,
        target,
        &sample_values,
    );
    if disposition == "adopted" && adopted_trial_passes_gates(&result) {
        result["reviewOutcome"] = json!("passed");
    }
    result
}

fn trial_decision(ledger: &[Value], target: i16, trial_index: i64) -> Value {
    let decision = ledger.iter().find(|item| {
        item.get("kind").and_then(Value::as_str) == Some("decision")
            && item.get("targetC").and_then(Value::as_i64) == Some(i64::from(target))
            && item.get("trialIndex").and_then(Value::as_i64) == Some(trial_index)
            && item.get("decision").is_some()
    });
    decision
        .and_then(|item| item.get("decision"))
        .cloned()
        .unwrap_or_else(|| json!({}))
}

fn append_trial_timing(
    result: &mut Value,
    trial_record: &Value,
    decision_record: &Value,
    target: i16,
    sample_values: &[&Value],
) {
    if let Some(object) = result.as_object_mut() {
        let trial_timing = trial_record.get("timing").filter(|value| !value.is_null());
        let decision_timing = decision_record
            .get("timing")
            .filter(|value| !value.is_null());
        if let Some(timing) = decision_timing.or(trial_timing) {
            object.insert(
                "timing".to_string(),
                if trial_timing == decision_timing {
                    timing.clone()
                } else {
                    json!({"availability": "mismatched_device_timing_evidence"})
                },
            );
        }
        if let Some(start) = trial_record.get("startSequence") {
            object.insert("trialStartSequence".to_string(), start.clone());
        }
        if let Some(end) = trial_record.get("endSequence") {
            object.insert("trialEndSequence".to_string(), end.clone());
        }
    }
    if let Some(timing) = result
        .get("timing")
        .and_then(|value| serde_json::from_value::<ThermalTuningTimingEvidence>(value.clone()).ok())
    {
        result["timingEvidenceVerification"] =
            verify_timing_samples(&timing, target, sample_values);
    }
}

fn trial_samples(
    samples: &[Value],
    target: i16,
    trial_index: i64,
    sample_start: Option<u64>,
    sample_end: Option<u64>,
) -> Vec<&Value> {
    samples
        .iter()
        .filter(|sample| {
            sample.get("targetC").and_then(Value::as_i64) == Some(i64::from(target))
                && sample.get("trialIndex").and_then(Value::as_i64) == Some(trial_index)
                && sample.get("phase").and_then(Value::as_str) != Some("cooldown")
                && sample
                    .get("sequence")
                    .and_then(Value::as_u64)
                    .is_some_and(|sequence| {
                        sample_start.is_none_or(|start| sequence >= start)
                            && sample_end.is_none_or(|end| sequence <= end)
                    })
        })
        .collect::<Vec<_>>()
}

fn sample_bounds_for_trial(samples: &[&Value]) -> (Value, Value, Value, Value) {
    let mut bounds: Option<(u64, u64, u64, u64)> = None;
    for sample in samples {
        let sequence = sample.get("sequence").and_then(Value::as_u64);
        let elapsed = sample.get("elapsedMs").and_then(Value::as_u64);
        let (Some(sequence), Some(elapsed)) = (sequence, elapsed) else {
            continue;
        };
        bounds = Some(match bounds {
            Some((first_sequence, last_sequence, first_elapsed, last_elapsed)) => (
                first_sequence.min(sequence),
                last_sequence.max(sequence),
                first_elapsed.min(elapsed),
                last_elapsed.max(elapsed),
            ),
            None => (sequence, sequence, elapsed, elapsed),
        });
    }
    bounds.map_or(
        (Value::Null, Value::Null, Value::Null, Value::Null),
        |(first, last, first_elapsed, last_elapsed)| {
            (
                json!(first),
                json!(last),
                json!(first_elapsed),
                json!(last_elapsed),
            )
        },
    )
}

fn phase_path(samples: &[&Value]) -> Value {
    let mut phases = Vec::<String>::new();
    for sample in samples {
        let Some(phase) = sample.get("phase").and_then(Value::as_str) else {
            continue;
        };
        if phases.last().map(String::as_str) != Some(phase) {
            phases.push(phase.to_string());
        }
    }
    json!(phases.join(" -> "))
}

fn collect_events(statuses: &[Value], samples: bool) -> Vec<Value> {
    let mut events = Vec::new();
    for status in statuses {
        let Some(values) = status.pointer("/events/events").and_then(Value::as_array) else {
            continue;
        };
        for event in values {
            let is_sample = event.get("kind").and_then(Value::as_str) == Some("sample");
            if is_sample != samples {
                continue;
            }
            let converted = if samples {
                flatten_sample_event(event)
            } else {
                Some(flatten_ledger_event(event))
            };
            if let Some(converted) = converted {
                events.push(converted);
            }
        }
    }
    events.sort_by_key(|event| {
        event
            .get("sequence")
            .and_then(Value::as_u64)
            .unwrap_or_default()
    });
    events.dedup_by_key(|event| event.get("sequence").and_then(Value::as_u64));
    events
}

fn flatten_sample_event(event: &Value) -> Option<Value> {
    let mut flat = event.get("sample")?.clone();
    if let Some(object) = flat.as_object_mut() {
        object.insert("kind".to_string(), json!("sample"));
        if !object.contains_key("sequence")
            && let Some(sequence) = event.get("sequence")
        {
            object.insert("sequence".to_string(), sequence.clone());
        }
        if let Some(contract) = object.remove("ppsContract") {
            for (target, source) in [
                ("ppsContractMv", "voltageMv"),
                ("ppsContractMa", "currentMa"),
                ("ppsContractKind", "kind"),
                ("ppsContractConfirmed", "confirmed"),
            ] {
                object.insert(
                    target.to_string(),
                    contract.get(source).cloned().unwrap_or(Value::Null),
                );
            }
        }
        if !object.contains_key("ppsRequestedMv")
            && let Some(requested) = object.remove("requestedMv")
        {
            object.insert("ppsRequestedMv".to_string(), requested);
        }
        if !object.contains_key("pidPhase")
            && let Some(phase) = object.get("phase").cloned()
        {
            object.insert("pidPhase".to_string(), phase);
        }
    }
    Some(flat)
}

fn flatten_ledger_event(event: &Value) -> Value {
    let mut ledger = event.clone();
    if let Some(object) = ledger.as_object_mut() {
        object.insert(
            "kind".to_string(),
            event.get("kind").cloned().unwrap_or(json!("safety")),
        );
    }
    ledger
}

fn write_ndjson(
    path: &PathBuf,
    values: &[Value],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut output = String::new();
    for value in values {
        output.push_str(&serde_json::to_string(value)?);
        output.push('\n');
    }
    fs::write(path, output)?;
    Ok(())
}

#[cfg(test)]
#[path = "thermal_firmware_tests.rs"]
mod tests;
