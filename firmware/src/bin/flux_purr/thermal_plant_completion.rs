#[allow(unused_imports)]
use super::*;

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Debug, Default)]
pub(crate) struct ThermalPlantCompletion {
    pub(crate) cutoff_reached: bool,
    pub(crate) failure_condition: Option<&'static str>,
    pub(crate) candidate: Option<Box<flux_purr_firmware::memory::ThermalPlantPersistence>>,
    pub(crate) fitting_started_tick: u32,
    pub(crate) persistence_started_tick: u32,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_failure_context(
    calibration: &CalibrationRuntimeState,
    job: &CalibrationThermalPlantAutoJob,
) -> Option<Box<flux_purr_firmware::control_plane::ThermalPlantFailureContextWire>> {
    if calibration.job.status != CalibrationJobStatus::Failed {
        return None;
    }
    let (stage, condition, started) = if job.completion.persistence_started_tick != 0 {
        (
            "persistence",
            "eeprom_write_and_readback_verified",
            job.completion.persistence_started_tick,
        )
    } else if job.completion.fitting_started_tick != 0 {
        (
            "fitting",
            "physical_fit_and_complete_trace",
            job.completion.fitting_started_tick,
        )
    } else {
        match job.phase {
            ThermalPlantAutoPhase::Ambient => {
                ("ambient", "valid_ambient_samples", job.phase_started_tick)
            }
            ThermalPlantAutoPhase::Heating => (
                "heating",
                "live_temperature_at_least_220c",
                job.phase_started_tick,
            ),
            ThermalPlantAutoPhase::Cooling => (
                "cooling",
                "passive_temperature_at_most_80c",
                job.phase_started_tick,
            ),
        }
    };
    let condition = job.completion.failure_condition.unwrap_or(
        if job.elapsed_ticks >= THERMAL_PLANT_TOTAL_TIMEOUT_TICKS {
            "total_elapsed_less_than_660000ms"
        } else {
            condition
        },
    );
    Some(Box::new(
        flux_purr_firmware::control_plane::ThermalPlantFailureContextWire {
            stage: heapless::String::try_from(stage).unwrap(),
            elapsed_ms: job
                .elapsed_ticks
                .saturating_mul(HEATER_CONTROL_INTERVAL_MS as u32),
            stage_elapsed_ms: job
                .elapsed_ticks
                .saturating_sub(started)
                .saturating_mul(HEATER_CONTROL_INTERVAL_MS as u32),
            condition: heapless::String::try_from(condition).unwrap(),
        },
    ))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_continuation_request(
    boundary: &flux_purr_firmware::heater_supply::SupplyBoundary,
    memory: &MemoryConfig,
    manual: &ManualPpsState,
    input: CalibrationJobUpdateInput,
) -> Option<(u16, u16)> {
    let range = manual.thermal_plant_pps_range()?;
    let (min_mv, max_mv, _) = manual.thermal_plant_source_limits()?;
    boundary
        .safe_point(
            flux_purr_firmware::heater_supply::BoundaryQuery {
                temperature_centi_c: round_to_i16(input.latest_temp_c * 100.0),
                current_rtd_mv: Some(input.latest_rtd_raw_adc_mv),
                current_request_mv: manual.target_mv,
                current_ma: 3_000,
                wifi_started: input.wifi_started?,
                source_min_mv: range.min_mv,
                source_max_mv: range.max_mv,
                operational_source_min_mv: min_mv.max(5_500).div_ceil(100) * 100,
                operational_source_max_mv: max_mv.min(21_000) / 100 * 100,
                source_max_ma: 3_000,
                source_power_limit_mw: manual.thermal_plant_source_power_limit_mw(),
            },
            |mv| projected_rtd_temperature_c(memory, mv).map(|t| round_to_i16(t * 100.0)),
        )
        .map(|point| (point.tracking_request_mv, point.request_current_ma))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn finish_thermal_plant_persistence(
    calibration: &mut CalibrationRuntimeState,
    manual: &mut ManualPpsState,
    job: &mut CalibrationThermalPlantAutoJob,
    memory: &mut MemoryConfig,
    now_ms: u64,
    verified: bool,
) -> bool {
    if calibration.job.kind != Some(CalibrationJobKind::ThermalPlant)
        || calibration.job.status != CalibrationJobStatus::Running
    {
        job.completion.candidate = None;
        return false;
    }
    job.elapsed_ticks = (now_ms.saturating_sub(job.started_at_ms.unwrap_or(now_ms))
        / HEATER_CONTROL_INTERVAL_MS)
        .min(u64::from(u32::MAX)) as u32;
    let candidate = job.completion.candidate.take();
    let within_budget = job.elapsed_ticks < THERMAL_PLANT_TOTAL_TIMEOUT_TICKS;
    if !verified || !within_budget || candidate.is_none() {
        fail_thermal_plant_calibration(
            calibration,
            manual,
            if within_budget {
                ManualPpsError::WriteFailed
            } else {
                ManualPpsError::ThermalPlantTimeout
            },
        );
        return false;
    }
    candidate.unwrap().apply_to_config(memory);
    calibration_job_complete(
        calibration,
        CalibrationJobKind::ThermalPlant,
        job.sample_count,
        None,
    );
    true
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_commit_pending_thermal_plant(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) -> bool {
    let elapsed_ms = elapsed_ms.max(
        Instant::now()
            .as_millis()
            .saturating_sub(state.runtime_started_ms),
    );
    let pending = state
        .thermal_plant_workspace
        .job
        .as_ref()
        .is_some_and(|job| job.completion.candidate.is_some());
    if !pending {
        return false;
    }
    if state.calibration_runtime_state.job.status != CalibrationJobStatus::Running {
        state
            .thermal_plant_workspace
            .job
            .as_mut()
            .unwrap()
            .completion
            .candidate = None;
        return true;
    }
    let job = state.thermal_plant_workspace.job.as_mut().unwrap();
    if elapsed_ms.saturating_sub(job.started_at_ms.unwrap_or(elapsed_ms)) >= 660_000 {
        finish_thermal_plant_persistence(
            &mut state.calibration_runtime_state,
            &mut state.manual_pps_state,
            job,
            &mut state.memory_config,
            elapsed_ms,
            false,
        );
        return true;
    }
    #[cfg(feature = "web_serial")]
    if state.transport.eeprom_snapshot_session.active {
        return true;
    }
    if !runtime_memory_commit_permitted(
        state.ui_state.heater_enabled,
        state.calibration_runtime_state.heater_enabled,
        state.manual_pps_state.enabled,
        state.ui_state.heater_output_percent,
        state.last_heater_duty,
    ) {
        return true;
    }
    let mut candidate_config = Box::new(state.memory_config.clone());
    job.completion
        .candidate
        .as_ref()
        .unwrap()
        .apply_to_config(&mut candidate_config);
    let commit_domains =
        persist_domain_mask_between(&candidate_config, &state.last_persisted_memory_config);
    let before = Instant::now().as_millis();
    let result = commit_memory_config_now(
        &mut state.eeprom_i2c,
        CommitMemoryConfigInput {
            deadline_ms: Some(
                state
                    .runtime_started_ms
                    .saturating_add(job.started_at_ms.unwrap_or(elapsed_ms))
                    .saturating_add(660_000),
            ),
            memory_sequence: &mut state.memory_sequence,
            memory_config: &candidate_config,
            domains_to_write: commit_domains,
            persistence_log_sink: &mut state.transport.persistence_log_sink,
            record_staging: state.eeprom_record_staging,
        },
    )
    .await;
    let finished_ms = elapsed_ms.saturating_add(Instant::now().as_millis().saturating_sub(before));
    runtime_finish_thermal_commit(
        state,
        &candidate_config,
        commit_domains,
        finished_ms,
        result,
    );
    true
}

#[cfg(target_arch = "xtensa")]
fn runtime_finish_thermal_commit(
    state: &mut RuntimeLoopState,
    candidate_config: &MemoryConfig,
    commit_domains: PersistDomainMask,
    finished_ms: u64,
    result: Result<u64, MemoryCommitFailure>,
) {
    let started_at_ms = state
        .thermal_plant_workspace
        .job
        .as_ref()
        .unwrap()
        .started_at_ms
        .unwrap_or(finished_ms);
    let saved_at_ms = result.as_ref().map_or(finished_ms, |verified_at_ms| {
        verified_at_ms.saturating_sub(state.runtime_started_ms)
    });
    let activated = finish_thermal_plant_persistence(
        &mut state.calibration_runtime_state,
        &mut state.manual_pps_state,
        state.thermal_plant_workspace.job.as_mut().unwrap(),
        &mut state.memory_config,
        saved_at_ms,
        result.is_ok(),
    );
    if activated {
        copy_persisted_domains(
            &mut state.last_persisted_memory_config,
            candidate_config,
            commit_domains,
        );
    } else {
        let error = result.err().unwrap_or(MemoryCommitFailure {
            error: MemoryCommitError::DeadlineExceeded,
            phase: "deadline",
            attempt: 1,
            sequence: state.memory_sequence,
            domain: PersistDomain::ThermalPlant,
            slot: PersistSlot::Single,
        });
        fail_pending_thermal_persistence(
            state,
            error,
            finished_ms.saturating_sub(started_at_ms) >= 660_000,
        );
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn continue_completed_supply_scan(
    scan: &mut flux_purr_firmware::heater_supply::scan::SupplyScan,
    output: &mut flux_purr_firmware::heater_supply::scan::ScanOutput,
    memory: &MemoryConfig,
    manual: &ManualPpsState,
    input: CalibrationJobUpdateInput,
) {
    if scan.phase != flux_purr_firmware::heater_supply::scan::ScanPhase::Complete {
        return;
    }
    if input.latest_temp_c >= THERMAL_PLANT_TARGET_TEMP_C {
        scan.cutoff_reached = true;
        scan.boundary.cutoff_reached = true;
        return;
    }
    output.heater_enabled = false;
    if let Some((mv, ma)) =
        thermal_plant_continuation_request(&scan.boundary, memory, manual, input)
    {
        output.request_mv = mv;
        output.request_ma = ma;
        output.heater_enabled = input.pd_confirmed
            && input.pd_conduction_allowed
            && manual.target_mv == Some(mv)
            && manual.target_ma == Some(ma);
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_stage_error(
    job: &CalibrationThermalPlantAutoJob,
    input: CalibrationJobUpdateInput,
) -> Option<ManualPpsError> {
    if job.elapsed_ticks >= THERMAL_PLANT_TOTAL_TIMEOUT_TICKS {
        return Some(ManualPpsError::ThermalPlantTimeout);
    }
    if job.completion.candidate.is_some() {
        return None;
    }
    let elapsed = job.elapsed_ticks.saturating_sub(job.phase_started_tick);
    let deadline = match job.phase {
        ThermalPlantAutoPhase::Ambient => return None,
        ThermalPlantAutoPhase::Heating => THERMAL_PLANT_HEAT_TIMEOUT_TICKS,
        ThermalPlantAutoPhase::Cooling => THERMAL_PLANT_COOL_TIMEOUT_TICKS,
    };
    let satisfied = match job.phase {
        ThermalPlantAutoPhase::Heating => input.latest_temp_c >= THERMAL_PLANT_TARGET_TEMP_C,
        ThermalPlantAutoPhase::Cooling => input.latest_temp_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C,
        ThermalPlantAutoPhase::Ambient => false,
    };
    if elapsed > deadline || (elapsed == deadline && !satisfied) {
        return Some(ManualPpsError::ThermalPlantTimeout);
    }
    if job
        .supply_scan
        .as_ref()
        .is_some_and(|scan| Some(scan.boundary.wifi_started) != input.wifi_started)
    {
        return Some(ManualPpsError::SupplyScan(
            flux_purr_firmware::heater_supply::scan::ScanFailure::WifiChanged,
        ));
    }
    None
}

#[cfg(target_arch = "xtensa")]
fn fail_pending_thermal_persistence(
    state: &mut RuntimeLoopState,
    error: MemoryCommitFailure,
    timed_out: bool,
) {
    // Writes may already have changed single-slot storage. Keep the established
    // persistence lock and the previous in-memory model for diagnosis.
    mark_eeprom_required(
        &mut state.ui_state,
        &mut state.calibration_runtime_state,
        &mut state.manual_pps_state,
        &mut state.memory_commit_due_ms,
        Some(persistence_fault_from_commit(error)),
    );
    state.calibration_runtime_state.job.status = CalibrationJobStatus::Failed;
    state.calibration_runtime_state.job.message = Some(if timed_out {
        ManualPpsError::ThermalPlantTimeout
    } else {
        ManualPpsError::WriteFailed
    });
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn enforce_thermal_plant_safety(
    calibration: &mut CalibrationRuntimeState,
    manual: &mut ManualPpsState,
    workspace: &mut CalibrationThermalPlantWorkspace,
    sensor_fault: bool,
    fan_enabled: bool,
) -> bool {
    if calibration.job.kind != Some(CalibrationJobKind::ThermalPlant)
        || calibration.job.status != CalibrationJobStatus::Running
    {
        return false;
    }
    let (error, condition) = if sensor_fault {
        (
            ManualPpsError::SupplyScan(
                flux_purr_firmware::heater_supply::scan::ScanFailure::InvalidSample,
            ),
            "rtd_sensor_healthy",
        )
    } else if fan_enabled {
        (
            ManualPpsError::ThermalPlantProjectionInvalid,
            "passive_fan_off",
        )
    } else {
        return false;
    };
    if let Some(job) = workspace.job.as_mut() {
        job.completion.failure_condition = Some(condition);
    }
    calibration_job_fail(calibration, error, true, manual);
    true
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_freeze_thermal_attempt_elapsed(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) {
    let now_ms = elapsed_ms.max(
        Instant::now()
            .as_millis()
            .saturating_sub(state.runtime_started_ms),
    );
    if let Some(job) = state.thermal_plant_workspace.job.as_mut() {
        job.elapsed_ticks = (now_ms.saturating_sub(job.started_at_ms.unwrap_or(now_ms))
            / HEATER_CONTROL_INTERVAL_MS)
            .min(u64::from(u32::MAX)) as u32;
        if job.elapsed_ticks >= THERMAL_PLANT_TOTAL_TIMEOUT_TICKS {
            fail_thermal_plant_calibration(
                &mut state.calibration_runtime_state,
                &mut state.manual_pps_state,
                ManualPpsError::ThermalPlantTimeout,
            );
        }
    }
}
