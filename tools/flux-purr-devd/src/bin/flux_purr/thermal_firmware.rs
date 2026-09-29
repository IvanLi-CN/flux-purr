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
        THERMAL_TUNING_EXECUTION_ORDER_C, THERMAL_TUNING_TARGETS_C, ThermalTuningEvent,
        digest_event_chain, hash_profile,
    },
};
use reqwest::{Client, Method};
use serde_json::{Value, json};

use super::{
    ResolvedUsbTarget, ThermalCandidateProfile, ThermalFirmwareSaveArgs, ThermalFlagshipTuneArgs,
    ThermalProfileMode, ThermalTuneEngine, create_lease, current_unix_millis,
    load_thermal_default_seed_candidate_profile, parse_thermal_targets, release_lease,
    request_leased, resolve_target, spawn_heartbeat, thermal_candidate_point,
    thermal_candidate_profile_from_value, thermal_candidate_profile_to_value,
    thermal_interpolated_candidate_point,
    thermal_tuning_v2_report::{self, ThermalTuningV2ReportInput},
};

const FIRMWARE_PHYSICAL_TARGETS_C: &str = "60,80,100,120,140,160,180,220,240";
const POLL_INTERVAL: Duration = Duration::from_millis(50);

struct StatusJournal {
    file: File,
    last_profile_hash: Option<String>,
}

impl StatusJournal {
    fn create(input_dir: &Path) -> io::Result<Self> {
        Ok(Self {
            file: fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(input_dir.join("device-statuses.ndjson"))?,
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
            last_profile_hash,
        })
    }

    fn record(&mut self, statuses: &mut Vec<Value>, mut status: Value) -> io::Result<()> {
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
    validate_firmware_args(&args)?;
    let resolved = resolve_target(args.target.clone(), default_devd)?;
    let pps3a_rejected = args.profile_mode == ThermalProfileMode::W65;
    if args.dry_run {
        return Ok(json!({
            "ok": true,
            "engine": "firmware",
            "powerClass": if pps3a_rejected { "pps3a" } else { "pps5a" },
            "targetsC": if pps3a_rejected { Vec::new() } else { THERMAL_TUNING_EXECUTION_ORDER_C.to_vec() },
            "device": resolved.device,
            "dryRun": true
        }));
    }
    if pps3a_rejected {
        return render_unsupported_pps3a(&resolved.device, &args.output_root, args.bundle_dir);
    }

    let run_id = format!("firmware-pps5a-{}", current_unix_millis());
    let candidate_id = format!("{run_id}-candidate");
    let seed_profile = if let Some(path) = args.seed_profile_file.as_ref() {
        let imported: Value = serde_json::from_slice(&fs::read(path)?)?;
        thermal_candidate_profile_from_value(imported)
    } else {
        let (profile, _) = load_thermal_default_seed_candidate_profile("pps5a")?;
        profile
    };
    let candidate_profile = materialize_firmware_seed_profile(seed_profile)?;
    let candidate_hash = firmware_candidate_profile_hash(&candidate_profile)?;
    let input_dir = args.output_root.join(&run_id);
    fs::create_dir_all(&input_dir)?;
    let mut journal = StatusJournal::create(&input_dir)?;
    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let mut statuses = Vec::new();
    let mut profile_mode_prepared = false;
    let result: Result<Value, Box<dyn std::error::Error + Send + Sync>> = async {
        ensure_firmware_profile_mode(client, &resolved, &lease.lease_id).await?;
        profile_mode_prepared = true;
        let start = match request_leased(
            client,
            &resolved,
            &lease.lease_id,
            Method::POST,
            "/thermal-tuning",
            Some(firmware_start_command(
                &run_id,
                &candidate_id,
                &candidate_hash,
                &candidate_profile,
            )),
        )
        .await
        {
            Ok(start) => start,
            Err(error) => {
                eprintln!("firmware thermal tuning start failed: {error}");
                let rejected = request_leased(
                    client,
                    &resolved,
                    &lease.lease_id,
                    Method::POST,
                    "/thermal-tuning",
                    Some(firmware_read_command(&run_id, None)),
                )
                .await;
                match rejected {
                    Ok(status)
                        if is_confirmed_rejected_start(
                            &status,
                            &run_id,
                            &candidate_id,
                            &candidate_hash,
                        ) =>
                    {
                        status
                    }
                    _ => {
                        let _ = request_leased(
                            client,
                            &resolved,
                            &lease.lease_id,
                            Method::POST,
                            "/thermal-tuning",
                            Some(json!({"op": "cancel"})),
                        )
                        .await;
                        return render_host_failure(HostFailureInput {
                            device_id: &resolved.device,
                            output_root: &args.output_root,
                            bundle_dir: args.bundle_dir.clone(),
                            run_id: Some(&run_id),
                            power_class: "pps5a",
                            reason: "start_not_confirmed",
                            no_device_command: false,
                            attempted_candidate: Some((
                                &candidate_id,
                                &candidate_hash,
                                &candidate_profile,
                            )),
                        });
                    }
                }
            }
        };

        let deadline = Instant::now()
            + Duration::from_secs(args.per_target_budget_seconds.saturating_mul(9).max(60));
        let mut after_sequence = next_event_sequence(&start);
        journal.record(&mut statuses, start)?;
        loop {
            let mut read_body = json!({"op": "read", "runId": run_id});
            if let Some(sequence) = after_sequence {
                read_body["afterSequence"] = json!(sequence);
            }
            let status = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::POST,
                "/thermal-tuning",
                Some(read_body),
            )
            .await
            .map_err(|error| {
                eprintln!("firmware thermal tuning event read failed: {error}");
                error
            })?;
            after_sequence = next_event_sequence(&status).or(after_sequence);
            let caught_up = event_stream_caught_up(&status, after_sequence);
            let terminal = status
                .get("terminalDisposition")
                .and_then(Value::as_str)
                .is_some_and(|value| {
                    matches!(value, "candidate_ready" | "passed" | "failed" | "canceled")
                });
            let trace_gap = status_trace_gap(&status);
            journal.record(&mut statuses, status)?;
            if terminal
                && caught_up
                && !trace_gap
                && let Some(sequence) = after_sequence
            {
                let confirmed = confirm_terminal_tuning_events(
                    client,
                    &resolved,
                    &lease.lease_id,
                    &run_id,
                    sequence,
                )
                .await?;
                journal.record(&mut statuses, confirmed)?;
            }
            if terminal && caught_up {
                break;
            }
            if Instant::now() >= deadline {
                return Err("firmware thermal tuning exceeded the bounded run budget".into());
            }
            if caught_up {
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }

        let candidate_hash = statuses
            .last()
            .and_then(|status| status.get("candidateHash"))
            .and_then(Value::as_str)
            .unwrap_or(&candidate_hash)
            .to_string();

        write_v2_input(V2Input {
            input_dir: &input_dir,
            device_id: &resolved.device,
            run_id: &run_id,
            candidate_id: &candidate_id,
            candidate_hash: &candidate_hash,
            candidate_profile: &candidate_profile,
            statuses: &statuses,
            host_interruption: None,
        })?;
        let mut rendered =
            thermal_tuning_v2_report::render_v2_bundle(ThermalTuningV2ReportInput {
                input_dir: input_dir.clone(),
                output_dir: args.bundle_dir.clone(),
            })?;
        rendered["rawInputDir"] = json!(input_dir);
        Ok(rendered)
    }
    .await;
    if result.is_err()
        && profile_mode_prepared
        && let Ok(canceled) = request_leased(
            client,
            &resolved,
            &lease.lease_id,
            Method::POST,
            "/thermal-tuning",
            Some(json!({"op": "cancel"})),
        )
        .await
        && let Err(error) = journal.record(&mut statuses, canceled)
    {
        eprintln!("thermal tuning status journal failed after cancel: {error}");
    }
    let rendered = match result {
        Ok(rendered) => Ok(rendered),
        Err(_) if !statuses.is_empty() => render_interrupted_run(InterruptedRunInput {
            device_id: &resolved.device,
            output_root: &args.output_root,
            bundle_dir: args.bundle_dir,
            run_id: &run_id,
            candidate_id: &candidate_id,
            seed_candidate_hash: &candidate_hash,
            candidate_profile: &candidate_profile,
            statuses: &statuses,
        }),
        Err(_) => render_host_failure(HostFailureInput {
            device_id: &resolved.device,
            output_root: &args.output_root,
            bundle_dir: args.bundle_dir,
            run_id: Some(&run_id),
            power_class: "pps5a",
            reason: if profile_mode_prepared {
                "transport_or_runtime_failure"
            } else {
                "profile_mode_not_confirmed"
            },
            no_device_command: false,
            attempted_candidate: Some((&candidate_id, &candidate_hash, &candidate_profile)),
        }),
    };
    let release_result = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();
    release_result?;
    rendered
}

fn firmware_profile_mode_ready(status: &Value) -> bool {
    status.get("thermalProfileMode").and_then(Value::as_str) == Some("100w")
        && status
            .get("thermalProfileResolvedBank")
            .and_then(Value::as_str)
            == Some("pps5a")
}

async fn ensure_firmware_profile_mode(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let status = request_leased(client, resolved, lease_id, Method::GET, "/status", None).await?;
    if firmware_profile_mode_ready(&status) {
        return Ok(());
    }
    if status.get("heaterEnabled").and_then(Value::as_bool) != Some(false) {
        return Err("heater must be off before selecting the 5A profile mode".into());
    }
    request_leased(
        client,
        resolved,
        lease_id,
        Method::PUT,
        "/runtime",
        Some(json!({"thermalProfileMode": "100w"})),
    )
    .await?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status =
            request_leased(client, resolved, lease_id, Method::GET, "/status", None).await?;
        if firmware_profile_mode_ready(&status) {
            return Ok(());
        }
        if status.get("heaterEnabled").and_then(Value::as_bool) != Some(false) {
            return Err("heater became armed while selecting the 5A profile mode".into());
        }
        if Instant::now() >= deadline {
            return Err("5A profile mode readback timed out".into());
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
    validate_save_input(&bundle, &candidate)?;
    let resolved = resolve_target(args.target, default_devd)?;
    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let result = async {
        let last_sequence = statuses
            .last()
            .and_then(|status| status.pointer("/trace/lastSequence"))
            .and_then(Value::as_u64);
        let current = request_leased(
            client,
            &resolved,
            &lease.lease_id,
            Method::POST,
            "/thermal-tuning",
            Some(firmware_read_command(run_id, last_sequence)),
        )
        .await?;
        if current.get("runId").and_then(Value::as_str) != Some(run_id)
            || current.get("candidateId").and_then(Value::as_str) != Some(candidate_id)
            || current.get("candidateHash").and_then(Value::as_str) != Some(candidate_hash)
            || current.get("terminalDisposition").and_then(Value::as_str) != Some("candidate_ready")
            || current.pointer("/trace/gap").and_then(Value::as_bool) != Some(false)
        {
            return Err("device candidate identity or trace changed before confirmation".into());
        }
        journal.record(&mut statuses, current)?;
        let sealed = request_leased(
            client,
            &resolved,
            &lease.lease_id,
            Method::POST,
            "/thermal-tuning",
            Some(json!({"op": "seal"})),
        )
        .await?;
        let seal_sequence = next_event_sequence(&sealed)
            .ok_or("sealed candidate did not emit an auditable transition")?;
        journal.record(&mut statuses, sealed)?;
        let seal_ack = request_leased(
            client,
            &resolved,
            &lease.lease_id,
            Method::POST,
            "/thermal-tuning",
            Some(firmware_read_command(run_id, Some(seal_sequence))),
        )
        .await?;
        if seal_ack.pointer("/trace/gap").and_then(Value::as_bool) != Some(false)
            || seal_ack
                .pointer("/trace/lastSequence")
                .and_then(Value::as_u64)
                != Some(seal_sequence)
        {
            return Err("sealed candidate transition was not acknowledged".into());
        }
        journal.record(&mut statuses, seal_ack)?;
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
                "powerClass": "pps5a"
            })),
        )
        .await?;
        if saved.get("terminalDisposition").and_then(Value::as_str) != Some("passed")
            || saved.get("saveReceipt").is_none_or(Value::is_null)
        {
            return Err("device did not return a verified save receipt".into());
        }
        journal.record(&mut statuses, saved)?;
        if let Some(sequence) = statuses.last().and_then(next_event_sequence) {
            let confirmed = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::POST,
                "/thermal-tuning",
                Some(firmware_read_command(run_id, Some(sequence))),
            )
            .await?;
            if confirmed.pointer("/trace/gap").and_then(Value::as_bool) != Some(false)
                || confirmed
                    .pointer("/trace/lastSequence")
                    .and_then(Value::as_u64)
                    != Some(sequence)
            {
                return Err("saved tuning receipt event was not confirmed".into());
            }
            journal.record(&mut statuses, confirmed)?;
        }
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

fn validate_save_input(
    bundle: &Value,
    candidate: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let run_id = required_string(bundle, "runId")?;
    let candidate_id = required_string(candidate, "candidateId")?;
    let candidate_hash = required_string(candidate, "candidateHash")?;
    if !default_5a_scope_passed(bundle)
        || bundle.get("terminalDisposition").and_then(Value::as_str) != Some("candidate_ready")
        || bundle
            .pointer("/deviceTraceVerification/verified")
            .and_then(Value::as_bool)
            != Some(true)
        || candidate.get("runId").and_then(Value::as_str) != Some(run_id)
        || candidate.pointer("/profile/bank").and_then(Value::as_str) != Some("pps5a")
        || candidate.get("powerClass").and_then(Value::as_str) != Some("pps5a")
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

fn default_5a_scope_passed(bundle: &Value) -> bool {
    let Ok(physical) = parse_thermal_targets(Some(FIRMWARE_PHYSICAL_TARGETS_C)) else {
        return false;
    };
    if bundle.get("schema").and_then(Value::as_str) != Some("thermal-tuning-v2")
        || bundle.get("engine").and_then(Value::as_str) != Some("firmware")
        || bundle.get("powerClass").and_then(Value::as_str) != Some("pps5a")
        || bundle.pointer("/targetSet/physicalC") != Some(&json!(physical))
        || bundle.pointer("/targetSet/selectedC") != Some(&json!(physical))
        || bundle.pointer("/targetSet/executionOrderC")
            != Some(&json!(THERMAL_TUNING_EXECUTION_ORDER_C))
        || bundle
            .pointer("/qualificationScope/kind")
            .and_then(Value::as_str)
            != Some("default-pps5a-full-batch")
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
    trial
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
            .and_then(Value::as_u64)
            .is_some_and(|value| value >= 60)
        && matches!((settle, limit), (Some(settle), Some(limit)) if settle <= limit)
        && trial
            .pointer("/metrics/phasePath")
            .and_then(Value::as_str)
            .is_some_and(|path| path.contains("approach") && path.contains("hold"))
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, io::Error> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("missing {field}")))
}

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
        host_interruption: Some("transport_or_runtime_failure"),
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
    } = input;
    let run_id = run_id
        .map(str::to_owned)
        .unwrap_or_else(|| format!("firmware-failed-{}", current_unix_millis()));
    let input_dir = output_root.join(&run_id);
    fs::create_dir_all(&input_dir)?;
    let bundle = json!({
        "schema": "thermal-tuning-v2",
        "runId": run_id,
        "engine": "firmware",
        "device": {"id": device_id, "transport": if no_device_command { "host-preflight-no-device-command" } else { "device-command-unconfirmed" }},
        "firmware": {"identity": null, "availability": if no_device_command { "device_not_queried" } else { "start_response_unavailable" }},
        "powerClass": power_class,
        "sourceClass": "unavailable",
        "ppsControlPolicyVersion": null,
        "ppsVoltageBounds": {"absoluteMinMv": null, "absoluteMaxMv": null, "policyFloorMv": null, "apdoMinMv": null, "apdoMaxMv": null, "availability": "device_not_queried"},
        "targetSet": {"physicalC": parse_thermal_targets(Some(FIRMWARE_PHYSICAL_TARGETS_C))?, "selectedC": [], "executionOrderC": []},
        "qualificationScope": {"kind": "host-failure", "targetCount": 0, "diagnosticOnly": true},
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
    let confirmed = request_leased(
        client,
        resolved,
        lease_id,
        Method::POST,
        "/thermal-tuning",
        Some(firmware_read_command(run_id, Some(sequence))),
    )
    .await?;
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
    status: &Value,
    run_id: &str,
    candidate_id: &str,
    candidate_hash: &str,
) -> bool {
    status.get("runId").and_then(Value::as_str) == Some(run_id)
        && status.get("candidateId").and_then(Value::as_str) == Some(candidate_id)
        && status.get("candidateHash").and_then(Value::as_str) == Some(candidate_hash)
        && status.get("powerClass").and_then(Value::as_str) == Some("pps5a")
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

fn firmware_start_command(
    run_id: &str,
    candidate_id: &str,
    candidate_hash: &str,
    candidate_profile: &Value,
) -> Value {
    json!({
        "op": "start",
        "runId": run_id,
        "candidateId": candidate_id,
        "candidateHash": candidate_hash,
        "powerClass": "pps5a",
        "targetsC": THERMAL_TUNING_EXECUTION_ORDER_C,
        "candidateProfile": {
            "op": "preview",
            "bank": "pps5a",
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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    if requested != expected {
        return Err("firmware engine requires the complete default PPS 5A target scope".into());
    }
    Ok(())
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
}

fn write_v2_input(input: V2Input<'_>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let V2Input {
        input_dir,
        device_id,
        run_id,
        candidate_id,
        candidate_hash,
        candidate_profile,
        statuses,
        host_interruption,
    } = input;
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
    let execution_order = THERMAL_TUNING_EXECUTION_ORDER_C;
    let report_runs = build_report_runs(&targets, &samples, &ledger, candidate_id, candidate_hash);
    let effective_candidate_profile = statuses
        .iter()
        .rev()
        .find_map(|status| {
            status
                .get("candidateProfile")
                .filter(|value| !value.is_null())
        })
        .unwrap_or(candidate_profile)
        .clone();
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
    let candidate_state = if verified_saved_receipt(
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
    let bundle = json!({
        "schema": "thermal-tuning-v2",
        "runId": run_id,
        "engine": "firmware",
        "device": {"id": device_id, "transport": "devd"},
        "firmware": {"source": "device_identity", "identity": "device_reported"},
        "powerClass": "pps5a",
        "sourceClass": "device-local",
        "ppsControlPolicyVersion": pps_policy_version,
        "ppsVoltageBounds": pps_voltage_bounds,
        "targetSet": {"physicalC": targets, "selectedC": targets, "executionOrderC": execution_order},
        "qualificationScope": {"kind": "default-pps5a-full-batch", "targetCount": targets.len(), "diagnosticOnly": false},
        "terminalDisposition": terminal,
        "reviewDisposition": if host_interruption.is_some() { json!("review_incomplete") } else { statuses.last().and_then(|status| status.get("reviewDisposition")).cloned().unwrap_or_else(|| json!("review_incomplete")) },
        "hostInterruption": host_interruption.map(|reason| json!({"reason": reason, "lastDeviceTerminalDisposition": device_terminal, "deviceState": "unconfirmed"})),
        "deviceTrace": device_trace,
        "deviceTraceVerification": device_trace_verification,
        "reportRuns": report_runs,
        "candidate": {
            "candidateId": candidate_id,
            "candidateHash": candidate_hash,
            "powerClass": "pps5a",
            "promotionState": candidate_state
        },
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
    let candidate = json!({
        "schema": "thermal-tuning-v2",
        "runId": run_id,
        "candidateId": candidate_id,
        "candidateHash": candidate_hash,
        "powerClass": "pps5a",
        "promotion": {
            "state": candidate_state,
            "previewReceipt": preview_receipt,
            "saveReceipt": save_receipt
        },
        "profile": {"bank": "pps5a", "source": "device_reported", "value": effective_candidate_profile}
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

fn verified_saved_receipt(
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
            && receipt.get("powerClass").and_then(Value::as_str) == Some("pps5a")
    }) && preview.get("ramAppliedHash").and_then(Value::as_str) == Some(candidate_hash)
        && save.get("bank").and_then(Value::as_str) == Some("pps5a")
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
            let adopted = trials
                .iter()
                .find(|trial| trial.get("disposition").and_then(Value::as_str) == Some("adopted"));
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
            json!({
                "targetC": target,
                "reviewOutcome": review_outcome,
                "reviewPassed": adopted.is_some(),
                "adoptedTrialIndex": adopted_trial_index,
                "trialCount": trials.len(),
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
            "powerClass": "pps5a"
        })
    });
    let decision = ledger.iter().find(|item| {
        item.get("kind").and_then(Value::as_str) == Some("decision")
            && item.get("targetC").and_then(Value::as_i64) == Some(i64::from(target))
            && item.get("trialIndex").and_then(Value::as_i64) == Some(trial_index)
            && item.get("decision").is_some()
    });
    let decision_record = decision
        .and_then(|item| item.get("decision"))
        .cloned()
        .unwrap_or_else(|| json!({}));
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
    let sample_values = samples
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
        .collect::<Vec<_>>();
    let bounds = sample_bounds_for_trial(&sample_values);
    let phases = phase_path(&sample_values);
    let warmup_request = sample_values
        .iter()
        .filter(|sample| sample.get("phase").and_then(Value::as_str) == Some("warmup"))
        .filter_map(|sample| {
            sample
                .get("heaterRequestedOutputPermille")
                .and_then(Value::as_u64)
        })
        .max();
    let settle_time = decision_record
        .get("settleTimeMs")
        .cloned()
        .unwrap_or(Value::Null);
    let limit_ms = decision_record
        .get("hardGateFullSpeedToStableMs")
        .cloned()
        .unwrap_or(Value::Null);
    let metrics = json!({
        "warmupRequestedOutputPermille": warmup_request,
        "phasePath": phases,
        "maxOvershootC": decision_record.get("maxOvershootCentiC").and_then(Value::as_f64).map(|value| value / 100.0),
        "holdPeakToPeakC": decision_record.get("holdPeakToPeakCentiC").and_then(Value::as_f64).map(|value| value / 100.0),
        "holdConfirmSeconds": if decision_record.get("holdConfirm").and_then(Value::as_bool) == Some(true) { json!(60) } else { Value::Null },
        "fullSpeedToStable": {"settleTimeMs": settle_time, "limitMs": limit_ms}
    });
    let mut result = json!({
        "trialIndex": trial_index,
        "candidateId": candidate.get("candidateId").cloned().unwrap_or_else(|| json!(fallback_candidate_id)),
        "candidateHash": candidate.get("candidateHash").cloned().unwrap_or_else(|| json!(fallback_candidate_hash)),
        "disposition": disposition,
        "reviewOutcome": if disposition == "adopted" { "passed" } else { "failed" },
        "metrics": metrics,
        "warmupRequestedOutputPermille": warmup_request,
        "maxOvershootC": decision_record.get("maxOvershootCentiC").and_then(Value::as_f64).map(|value| value / 100.0),
        "holdPeakToPeakC": decision_record.get("holdPeakToPeakCentiC").and_then(Value::as_f64).map(|value| value / 100.0),
        "fullSpeedToStable": {"settleTimeMs": settle_time, "limitMs": limit_ms},
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
    if let Some(object) = result.as_object_mut() {
        if let Some(start) = trial_record.get("startSequence") {
            object.insert("trialStartSequence".to_string(), start.clone());
        }
        if let Some(end) = trial_record.get("endSequence") {
            object.insert("trialEndSequence".to_string(), end.clone());
        }
    }
    result
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
mod tests {
    use super::{
        InterruptedRunInput, StatusJournal, build_report_runs, build_report_trial,
        event_stream_caught_up, firmware_candidate_profile_hash, firmware_candidate_profile_value,
        firmware_profile_mode_ready, firmware_read_command, firmware_start_command,
        is_confirmed_rejected_start, next_event_sequence, render_host_failure,
        render_interrupted_run, render_unsupported_pps3a, trial_events_for_target,
        validate_save_input, verified_saved_receipt, verify_device_trace,
    };
    use flux_purr_firmware::{
        control_plane::ThermalControlProfileWire,
        memory::ThermalControlProfileConfig,
        thermal_tuning::{ThermalTuningEvent, digest_event_chain, hash_profile},
    };
    use serde_json::json;

    #[test]
    fn firmware_tuning_requires_5a_mode_and_resolved_bank() {
        assert!(!firmware_profile_mode_ready(&json!({
            "thermalProfileMode": "65w",
            "thermalProfileResolvedBank": "pps3a"
        })));
        assert!(!firmware_profile_mode_ready(&json!({
            "thermalProfileMode": "100w",
            "thermalProfileResolvedBank": "pps3a"
        })));
        assert!(firmware_profile_mode_ready(&json!({
            "thermalProfileMode": "100w",
            "thermalProfileResolvedBank": "pps5a"
        })));
    }

    #[test]
    fn journal_persists_each_event_page_before_the_next_ack() {
        let directory = tempfile::tempdir().unwrap();
        let mut journal = StatusJournal::create(directory.path()).unwrap();
        let mut statuses = Vec::new();
        let first = json!({
            "candidateHash": "hash-1",
            "candidateProfile": {"points": [60]},
            "terminalDisposition": "running",
            "trace": {"lastSequence": 0, "complete": false, "gap": false, "digest": "digest-0"},
            "events": {"events": [{"sequence": 0}], "nextSequence": 0}
        });
        journal.record(&mut statuses, first.clone()).unwrap();
        let mut idle = first.clone();
        idle["events"] = json!({"events": [], "nextSequence": null});
        journal.record(&mut statuses, idle).unwrap();
        assert_eq!(statuses.len(), 1);

        let mut terminal = first;
        terminal["terminalDisposition"] = json!("candidate_ready");
        terminal["trace"] =
            json!({"lastSequence": 1, "complete": true, "gap": false, "digest": "digest-1"});
        terminal["events"] = json!({"events": [{"sequence": 1}], "nextSequence": 1});
        journal.record(&mut statuses, terminal.clone()).unwrap();
        assert!(statuses[1].get("candidateProfile").is_none());

        terminal["events"] = json!({"events": [], "nextSequence": null});
        journal.record(&mut statuses, terminal).unwrap();
        assert_eq!(statuses.len(), 3);
        let recorded =
            std::fs::read_to_string(directory.path().join("device-statuses.ndjson")).unwrap();
        let from_disk: Vec<serde_json::Value> = recorded
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(from_disk, statuses);

        drop(journal);
        let mut journal = StatusJournal::resume(directory.path(), &statuses).unwrap();
        let sealed = json!({
            "candidateHash": "hash-1",
            "candidateProfile": {"points": [60]},
            "sealed": true,
            "terminalDisposition": "candidate_ready",
            "trace": {"lastSequence": 2, "complete": true, "gap": false, "digest": "digest-2"},
            "events": {"events": [{"sequence": 2}], "nextSequence": 2}
        });
        journal.record(&mut statuses, sealed).unwrap();
        assert!(statuses.last().unwrap().get("candidateProfile").is_none());
        let recorded =
            std::fs::read_to_string(directory.path().join("device-statuses.ndjson")).unwrap();
        let from_disk: Vec<serde_json::Value> = recorded
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(from_disk, statuses);
    }

    #[test]
    fn journal_keeps_profile_returned_after_event_pages() {
        let directory = tempfile::tempdir().unwrap();
        let mut journal = StatusJournal::create(directory.path()).unwrap();
        let mut statuses = Vec::new();
        journal
            .record(
                &mut statuses,
                json!({"candidateHash": "hash-2", "events": {"events": []}, "trace": {"lastSequence": 2}}),
            )
            .unwrap();
        journal
            .record(
                &mut statuses,
                json!({"candidateHash": "hash-2", "candidateProfile": {"points": [60]}, "events": {"events": []}, "trace": {"lastSequence": 2}}),
            )
            .unwrap();
        journal
            .record(
                &mut statuses,
                json!({"candidateHash": "hash-2", "candidateProfile": {"points": [60]}, "events": {"events": []}, "trace": {"lastSequence": 2}}),
            )
            .unwrap();
        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses[1]["candidateProfile"]["points"], json!([60]));
    }

    #[test]
    fn read_ack_is_bound_to_the_run_identity() {
        assert_eq!(
            firmware_read_command("run-1", Some(17)),
            json!({"op": "read", "runId": "run-1", "afterSequence": 17})
        );
    }

    #[test]
    fn saved_promotion_requires_matching_device_receipts_even_if_review_fails() {
        let preview = json!({
            "runId": "run-1", "candidateId": "candidate-1", "candidateHash": "hash-1",
            "powerClass": "pps5a", "ramAppliedHash": "hash-1"
        });
        let mut save = json!({
            "runId": "run-1", "candidateId": "candidate-1", "candidateHash": "hash-1",
            "powerClass": "pps5a", "bank": "pps5a", "bankRevision": 3,
            "bankHash": "hash-1", "atomic": true, "readbackVerified": true
        });
        assert!(verified_saved_receipt(
            "run-1",
            "candidate-1",
            "hash-1",
            Some(&preview),
            Some(&save)
        ));
        save["bankHash"] = json!("other");
        assert!(!verified_saved_receipt(
            "run-1",
            "candidate-1",
            "hash-1",
            Some(&preview),
            Some(&save)
        ));
    }

    #[test]
    fn explicit_save_requires_verified_candidate_and_matching_preview_receipt() {
        let physical = [60, 80, 100, 120, 140, 160, 180, 220, 240];
        let runs = physical
            .iter()
            .map(|target| json!({
                "targetC": target,
                "reviewOutcome": "passed",
                "reviewPassed": true,
                "adoptedTrialIndex": 1,
                "trials": [{
                    "trialIndex": 1,
                    "disposition": "adopted",
                    "warmupRequestedOutputPermille": 1000,
                    "maxOvershootC": 1.2,
                    "holdPeakToPeakC": 1.8,
                    "fullSpeedToStable": {"settleTimeMs": 7000, "limitMs": 10000},
                    "metrics": {"holdConfirmSeconds": 60, "phasePath": "warmup -> approach -> hold"}
                }]
            }))
            .collect::<Vec<_>>();
        let bundle = json!({
            "schema": "thermal-tuning-v2",
            "runId": "run-1",
            "engine": "firmware",
            "powerClass": "pps5a",
            "targetSet": {
                "physicalC": physical,
                "selectedC": physical,
                "executionOrderC": [60, 240, 140, 100, 80, 120, 180, 160, 220]
            },
            "qualificationScope": {"kind": "default-pps5a-full-batch", "diagnosticOnly": false},
            "reportRuns": runs,
            "terminalDisposition": "candidate_ready",
            "deviceTraceVerification": {"verified": true},
            "candidate": {"candidateId": "candidate-1", "candidateHash": "hash-1"}
        });
        let candidate = json!({
            "runId": "run-1",
            "candidateId": "candidate-1",
            "candidateHash": "hash-1",
            "powerClass": "pps5a",
            "profile": {"bank": "pps5a"},
            "promotion": {
                "previewReceipt": {
                    "runId": "run-1", "candidateId": "candidate-1",
                    "candidateHash": "hash-1", "ramAppliedHash": "hash-1"
                },
                "saveReceipt": null
            }
        });
        assert!(validate_save_input(&bundle, &candidate).is_ok());
        let mut invalid = bundle.clone();
        invalid["deviceTraceVerification"]["verified"] = json!(false);
        assert!(validate_save_input(&invalid, &candidate).is_err());
        invalid = bundle.clone();
        invalid["terminalDisposition"] = json!("failed");
        assert!(validate_save_input(&invalid, &candidate).is_err());
        let mut invalid_candidate = candidate.clone();
        invalid_candidate["promotion"]["previewReceipt"]["ramAppliedHash"] = json!("other");
        assert!(validate_save_input(&bundle, &invalid_candidate).is_err());
        invalid_candidate = candidate.clone();
        invalid_candidate["promotion"]["saveReceipt"] = json!({"bankHash": "hash-1"});
        assert!(validate_save_input(&bundle, &invalid_candidate).is_err());
        invalid_candidate = candidate.clone();
        invalid_candidate["profile"]["bank"] = json!("pps3a");
        assert!(validate_save_input(&bundle, &invalid_candidate).is_err());

        invalid = bundle.clone();
        invalid["reportRuns"][0]["trials"][0]["disposition"] = json!("rejected");
        assert!(validate_save_input(&invalid, &candidate).is_err());
        invalid = bundle.clone();
        invalid["reportRuns"][0]["trials"][0]["metrics"]["holdConfirmSeconds"] = json!(12);
        assert!(validate_save_input(&invalid, &candidate).is_err());
        invalid = bundle.clone();
        invalid["targetSet"]["selectedC"] = json!([60]);
        assert!(validate_save_input(&invalid, &candidate).is_err());
    }

    #[test]
    fn start_page_advances_ack_and_terminal_waits_for_full_trace() {
        let start = json!({"events": {"nextSequence": 3}, "trace": {"lastSequence": 3}});
        assert_eq!(next_event_sequence(&start), Some(3));
        assert!(event_stream_caught_up(&start, Some(3)));

        let terminal_with_backlog = json!({
            "terminalDisposition": "failed",
            "events": {"nextSequence": 7},
            "trace": {"lastSequence": 12}
        });
        assert!(!event_stream_caught_up(&terminal_with_backlog, Some(7)));
        assert!(event_stream_caught_up(&terminal_with_backlog, Some(12)));
    }

    #[test]
    fn terminal_trace_gap_does_not_wait_for_an_unretained_event() {
        let gap_with_backlog = json!({
            "terminalDisposition": "failed",
            "events": {"events": [{"sequence": 320}], "nextSequence": 320},
            "trace": {"lastSequence": 321, "gap": true}
        });
        assert!(!event_stream_caught_up(&gap_with_backlog, Some(320)));
        let drained_gap = json!({
            "terminalDisposition": "failed",
            "events": {"events": [], "nextSequence": null},
            "trace": {"lastSequence": 321, "gap": true}
        });
        assert!(event_stream_caught_up(&drained_gap, Some(320)));
    }

    #[test]
    fn firmware_start_preserves_recursive_target_order() {
        let command = firmware_start_command("run-1", "candidate-1", "hash-1", &json!({}));
        assert_eq!(
            command["targetsC"],
            json!([60, 240, 140, 100, 80, 120, 180, 160, 220])
        );
        assert_eq!(command["candidateProfile"]["bank"], "pps5a");
    }

    #[test]
    fn interrupted_run_keeps_collected_device_samples_and_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let output_root = dir.path().join("input");
        let report_dir = dir.path().join("report");
        let statuses = vec![json!({
            "terminalDisposition": "running",
            "reviewDisposition": "pending",
            "candidateHash": "hash-device",
            "ppsControlPolicyVersion": "hold-pps-governor-v1",
            "ppsVoltageBounds": {
                "absoluteMinMv": 5000, "absoluteMaxMv": 28000,
                "policyFloorMv": 12000, "apdoMinMv": 11000, "apdoMaxMv": 21000
            },
            "trace": {"firstSequence": 0, "lastSequence": 0, "complete": false, "gap": false, "digest": "unconfirmed"},
            "events": {"events": [{
                "sequence": 0, "elapsedMs": 0, "kind": "sample", "phase": "warmup",
                "targetC": 60, "trialIndex": 0, "reason": null, "trial": null, "decision": null,
                "sample": {
                    "sequence": 0, "elapsedMs": 0, "targetC": 60, "trialIndex": 0,
                    "candidateId": "candidate-1", "candidateHash": "hash-device",
                    "temperatureCentiC": 3500, "vinMv": 12000,
                    "ppsContract": {"voltageMv": 12000, "currentMa": 5000, "kind": "pps5a", "confirmed": true},
                    "requestedMv": 12000, "ppsPowerCeilingMv": 21000,
                    "heaterRequestedOutputPermille": 1000, "heaterOutputPermille": 980,
                    "measurementValid": true, "phase": "warmup"
                }
            }]}
        })];
        let rendered = render_interrupted_run(InterruptedRunInput {
            device_id: "device-1",
            output_root: &output_root,
            bundle_dir: Some(report_dir.clone()),
            run_id: "run-1",
            candidate_id: "candidate-1",
            seed_candidate_hash: "hash-seed",
            candidate_profile: &json!({"settings": null, "points": []}),
            statuses: &statuses,
        })
        .unwrap();

        let bundle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report_dir.join("run.bundle.json")).unwrap())
                .unwrap();
        let samples = std::fs::read_to_string(report_dir.join("samples.ndjson")).unwrap();
        assert_eq!(rendered["ownerOutcome"], "failed");
        assert_eq!(bundle["terminalDisposition"], "failed");
        assert_eq!(bundle["reviewDisposition"], "review_incomplete");
        assert_eq!(
            bundle["hostInterruption"]["lastDeviceTerminalDisposition"],
            "running"
        );
        assert_eq!(bundle["candidate"]["candidateHash"], "hash-device");
        assert_eq!(bundle["ppsControlPolicyVersion"], "hold-pps-governor-v1");
        assert_eq!(bundle["ppsVoltageBounds"]["apdoMaxMv"], 21000);
        assert_eq!(bundle["ppsVoltageBounds"]["availability"], "device_local");
        assert!(samples.contains("\"temperatureCentiC\":3500"));
    }

    #[test]
    fn closed_candidate_trial_replaces_its_open_event() {
        let ledger = vec![
            json!({"kind": "candidate_trial", "targetC": 60, "trialIndex": 0, "trial": {"startSequence": 1, "endSequence": null}}),
            json!({"kind": "candidate_trial", "targetC": 60, "trialIndex": 0, "trial": {"startSequence": 1, "endSequence": 9}}),
            json!({"kind": "candidate_trial", "targetC": 60, "trialIndex": 1, "trial": {"startSequence": 10, "endSequence": null}}),
        ];
        let trials = trial_events_for_target(&ledger, 60);
        assert_eq!(trials.len(), 2);
        assert_eq!(trials[0]["trial"]["endSequence"], 9);
        assert!(trials[1]["trial"]["endSequence"].is_null());
    }

    #[test]
    fn target_without_an_adopted_trial_cannot_inherit_the_device_passed_terminal() {
        let runs = build_report_runs(&[60], &[], &[], "candidate-1", "hash-1");
        assert_eq!(runs[0]["reviewOutcome"], "failed");
        assert_eq!(runs[0]["reviewPassed"], false);
        assert_eq!(runs[0]["trialCount"], 0);
    }

    #[test]
    fn trial_report_excludes_cooldown_and_other_phase_warmup_requests() {
        let trial = json!({
            "targetC": 60, "trialIndex": 1,
            "trial": {
                "sampleStartSequence": 2, "sampleEndSequence": 4,
                "candidate": {"candidateId": "candidate-1", "candidateHash": "hash-1"}
            }
        });
        let samples = vec![
            json!({"sequence": 1, "targetC": 60, "trialIndex": 1, "phase": "cooldown", "elapsedMs": 1000, "heaterRequestedOutputPermille": 1000}),
            json!({"sequence": 2, "targetC": 60, "trialIndex": 1, "phase": "warmup", "elapsedMs": 2000, "heaterRequestedOutputPermille": 995}),
            json!({"sequence": 3, "targetC": 60, "trialIndex": 1, "phase": "approach", "elapsedMs": 3000, "heaterRequestedOutputPermille": 1000}),
            json!({"sequence": 4, "targetC": 60, "trialIndex": 1, "phase": "hold", "elapsedMs": 4000, "heaterRequestedOutputPermille": 400}),
            json!({"sequence": 5, "targetC": 60, "trialIndex": 1, "phase": "cooldown", "elapsedMs": 5000, "heaterRequestedOutputPermille": 1000}),
        ];
        let report = build_report_trial(60, &trial, &samples, &[], "candidate-1", "hash-1");
        assert_eq!(report["warmupRequestedOutputPermille"], 995);
        assert_eq!(report["firstSequence"], 2);
        assert_eq!(report["lastSequence"], 4);
        assert_eq!(report["metrics"]["phasePath"], "warmup -> approach -> hold");
    }

    #[test]
    fn verifies_device_event_digest_and_rejects_missing_or_changed_evidence() {
        let event: ThermalTuningEvent = serde_json::from_value(json!({
            "sequence": 0,
            "elapsedMs": 0,
            "kind": "safety",
            "phase": "failed",
            "targetC": null,
            "trialIndex": null,
            "sample": null,
            "reason": "pps3a_not_supported",
            "trial": null,
            "decision": null
        }))
        .unwrap();
        let digest = digest_event_chain(&[event.clone()]);
        assert_eq!(
            digest.as_str(),
            "4ea3bc9c23cbf417bbcd6468998d28c493745c28305d92ead01274c3a2f4f9d3"
        );
        let mut status = json!({
            "events": {"events": [serde_json::to_value(&event).unwrap()]},
            "trace": {"firstSequence": 0, "lastSequence": 0, "complete": true, "gap": false, "digest": digest.as_str()}
        });
        assert_eq!(verify_device_trace(&[status.clone()]).1["verified"], true);

        status["trace"]["digest"] = json!("wrong");
        assert_eq!(verify_device_trace(&[status.clone()]).1["verified"], false);

        status["trace"]["digest"] = json!(digest.as_str());
        status["trace"]["lastSequence"] = json!(1);
        assert_eq!(verify_device_trace(&[status]).1["verified"], false);
    }

    #[test]
    fn default_5a_candidate_hash_matches_firmware_sanitized_profile() {
        let (seed, _) = super::load_thermal_default_seed_candidate_profile("pps5a").unwrap();
        let profile =
            firmware_candidate_profile_value(&super::thermal_candidate_profile_to_value(&seed));
        assert_eq!(profile["points"].as_array().unwrap().len(), 10);
        let host_hash = firmware_candidate_profile_hash(&profile).unwrap();
        let mut wire_value = profile.clone();
        let points = wire_value["points"].as_array_mut().unwrap();
        while points.len() < flux_purr_firmware::frontpanel::FRONTPANEL_PRESET_COUNT {
            points.push(serde_json::Value::Null);
        }
        let wire: ThermalControlProfileWire = serde_json::from_value(wire_value).unwrap();
        let mut config: ThermalControlProfileConfig = wire.into();
        config.sanitize();
        assert_eq!(hash_profile(&config).unwrap().as_str(), host_hash);
    }

    #[test]
    fn default_5a_seed_materializes_every_firmware_target() {
        let (seed, _) = super::load_thermal_default_seed_candidate_profile("pps5a").unwrap();
        let profile = super::materialize_firmware_seed_profile(seed).unwrap();
        let targets: Vec<i16> = profile["points"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|point| point.get("targetTempC").and_then(serde_json::Value::as_i64))
            .map(|target| target as i16)
            .collect();
        assert_eq!(targets, vec![60, 80, 100, 120, 140, 160, 180, 220, 240]);
        assert_eq!(profile["points"].as_array().unwrap().len(), 10);
        assert!(super::firmware_candidate_profile_hash(&profile).is_ok());
    }

    #[test]
    fn pps3a_preflight_creates_a_five_file_failure_without_a_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let report_dir = dir.path().join("report");
        let result = render_unsupported_pps3a(
            "test-device",
            &dir.path().join("input"),
            Some(report_dir.clone()),
        )
        .unwrap();
        assert_eq!(result["ownerOutcome"], "failed");
        assert_eq!(result["trace"]["complete"], true);
        let bundle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report_dir.join("run.bundle.json")).unwrap())
                .unwrap();
        assert_eq!(bundle["powerClass"], "pps3a");
        assert_eq!(bundle["candidate"]["candidateId"], serde_json::Value::Null);
        assert_eq!(bundle["traceOrigin"], "host_preflight");
        assert_eq!(bundle["eventCounts"]["safety"], 1);
        assert_eq!(
            std::fs::read_to_string(report_dir.join("samples.ndjson")).unwrap(),
            ""
        );
    }

    #[test]
    fn unconfirmed_start_keeps_attempted_identity_and_never_qualifies() {
        let dir = tempfile::tempdir().unwrap();
        let report_dir = dir.path().join("report");
        let profile = json!({"points": []});
        let result = render_host_failure(super::HostFailureInput {
            device_id: "test-device",
            output_root: &dir.path().join("input"),
            bundle_dir: Some(report_dir.clone()),
            run_id: Some("attempted-run"),
            power_class: "pps5a",
            reason: "start_not_confirmed",
            no_device_command: false,
            attempted_candidate: Some(("attempted-candidate", "sha256:attempted", &profile)),
        })
        .unwrap();
        assert_eq!(result["ownerOutcome"], "failed");
        assert_eq!(result["trace"]["complete"], false);
        let bundle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report_dir.join("run.bundle.json")).unwrap())
                .unwrap();
        assert_eq!(bundle["runId"], "attempted-run");
        assert_eq!(bundle["candidate"]["candidateId"], "attempted-candidate");
        assert_eq!(bundle["rejectionReason"], "start_not_confirmed");
        let candidate: serde_json::Value = serde_json::from_slice(
            &std::fs::read(report_dir.join("thermal-profile.candidate.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(candidate["profile"], profile);
        assert_eq!(candidate["promotion"]["state"], "unavailable");
    }

    #[test]
    fn rejected_start_requires_matching_device_identity_and_disarmed_failure() {
        let mut status = json!({
            "runId": "run-1",
            "candidateId": "candidate-1",
            "candidateHash": "hash-1",
            "powerClass": "pps5a",
            "terminalDisposition": "failed",
            "armed": false
        });
        assert!(is_confirmed_rejected_start(
            &status,
            "run-1",
            "candidate-1",
            "hash-1"
        ));
        status["candidateHash"] = json!("other-hash");
        assert!(!is_confirmed_rejected_start(
            &status,
            "run-1",
            "candidate-1",
            "hash-1"
        ));
        status["candidateHash"] = json!("hash-1");
        status["armed"] = json!(true);
        assert!(!is_confirmed_rejected_start(
            &status,
            "run-1",
            "candidate-1",
            "hash-1"
        ));
    }
}
