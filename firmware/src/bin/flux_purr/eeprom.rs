#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
pub(crate) fn memory_commit_error_from_eeprom<I2cError>(
    error: EepromError<I2cError>,
) -> MemoryCommitError
where
    I2cError: embedded_hal::i2c::Error,
{
    match error {
        EepromError::OutOfRange
        | EepromError::PageWriteTooLong
        | EepromError::PageBoundaryCrossed => MemoryCommitError::WriteFailed,
        EepromError::I2c(error) => match error.kind() {
            embedded_hal::i2c::ErrorKind::NoAcknowledge(source) => match source {
                embedded_hal::i2c::NoAcknowledgeSource::Address => {
                    MemoryCommitError::WriteAddressNoAck
                }
                embedded_hal::i2c::NoAcknowledgeSource::Data => MemoryCommitError::WriteDataNoAck,
                _ => MemoryCommitError::WriteUnknownNoAck,
            },
            embedded_hal::i2c::ErrorKind::Bus => MemoryCommitError::WriteBus,
            embedded_hal::i2c::ErrorKind::ArbitrationLoss => MemoryCommitError::WriteArbitration,
            _ => MemoryCommitError::WriteOther,
        },
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn probe_eeprom_address(i2c: &mut I2c<'_>) -> Option<u8> {
    let mut eeprom = M24c64::with_address(i2c, M24C64_I2C_ADDRESS);
    let mut byte = [0u8; 1];
    eeprom
        .read_bytes_async(0, &mut byte)
        .await
        .ok()
        .map(|()| M24C64_I2C_ADDRESS)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn read_eeprom_bytes_chunked(
    i2c: &mut I2c<'_>,
    address: u8,
    offset: u16,
    bytes: &mut [u8],
) -> Result<(), ()> {
    let mut read = 0usize;
    while read < bytes.len() {
        let chunk_len = (bytes.len() - read).min(EEPROM_READ_CHUNK_MAX_BYTES);
        let chunk_offset = offset.checked_add(read as u16).ok_or(())?;
        let result = {
            let mut eeprom = M24c64::with_address(&mut *i2c, address);
            eeprom
                .read_bytes_async(chunk_offset, &mut bytes[read..read + chunk_len])
                .await
        };
        result.map_err(|_| ())?;
        read += chunk_len;
        EmbassyTimer::after_millis(0).await;
    }
    Ok(())
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn eeprom_bytes_contain_data(bytes: &[u8]) -> bool {
    bytes.iter().any(|byte| *byte != 0xff)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn write_eeprom_bytes_verified(
    i2c: &mut I2c<'_>,
    offset: u16,
    bytes: &[u8],
) -> Result<(), MemoryCommitError> {
    let Some(address) = probe_eeprom_address(i2c).await else {
        return Err(MemoryCommitError::WriteAddressNoAck);
    };
    let mut written = 0usize;
    while written < bytes.len() {
        let absolute_offset = usize::from(offset) + written;
        let chunk_len = eeprom_maintenance_write_chunk_len(absolute_offset, bytes.len() - written);
        let chunk_offset =
            u16::try_from(absolute_offset).map_err(|_| MemoryCommitError::WriteFailed)?;
        let write_result = {
            let mut eeprom = M24c64::with_address(&mut *i2c, address);
            eeprom
                .write_page_async(chunk_offset, &bytes[written..written + chunk_len])
                .await
        };
        write_result.map_err(memory_commit_error_from_eeprom)?;
        EmbassyTimer::after_millis(EEPROM_WRITE_CYCLE_DELAY_MS).await;
        written += chunk_len;
        EmbassyTimer::after_millis(0).await;
    }
    let mut verify = [0u8; flux_purr_firmware::control_plane::EEPROM_MAINTENANCE_CHUNK_MAX];
    let mut read = 0usize;
    while read < bytes.len() {
        let chunk_len = (bytes.len() - read).min(EEPROM_READ_CHUNK_MAX_BYTES);
        let chunk_offset = offset
            .checked_add(read as u16)
            .ok_or(MemoryCommitError::VerifyUnreadable)?;
        let read_result = {
            let mut eeprom = M24c64::with_address(&mut *i2c, address);
            eeprom
                .read_bytes_async(chunk_offset, &mut verify[read..read + chunk_len])
                .await
        };
        read_result.map_err(|_| MemoryCommitError::VerifyUnreadable)?;
        read += chunk_len;
        EmbassyTimer::after_millis(0).await;
    }
    if verify[..bytes.len()] != *bytes {
        return Err(MemoryCommitError::VerifyMismatch);
    }
    Ok(())
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn eeprom_maintenance_write_chunk_len(
    absolute_offset: usize,
    remaining: usize,
) -> usize {
    let page_size = flux_purr_firmware::memory::M24C64_PAGE_SIZE;
    let page_room = page_size - (absolute_offset % page_size);
    remaining.min(page_room).min(EEPROM_WRITE_CHUNK_MAX_BYTES)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn eeprom_data_is_incompatible(has_valid_record: bool, contains_data: bool) -> bool {
    !has_valid_record && contains_data
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn raw_eeprom_operation_mutates(op: EepromMaintenanceOp) -> bool {
    matches!(op, EepromMaintenanceOp::Write | EepromMaintenanceOp::Erase)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn begin_mutating_eeprom_maintenance(
    ui_state: &mut FrontPanelUiState,
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
    memory_commit_due_ms: &mut Option<u64>,
) {
    // The EEPROM may already be partially changed when the first I2C failure
    // is reported. Lock power and suppress normal record writes before sending
    // the first raw byte.
    ui_state.heater_enabled = false;
    ui_state.heater_output_percent = 0;
    ui_state.eeprom_data_incompatible = true;
    calibration_job_canceled(calibration, manual_pps);
    calibration.mode = CalibrationMode::Off;
    calibration.pps_enabled = false;
    calibration.pps_mv = None;
    calibration.pps_ma = None;
    calibration.heater_enabled = false;
    calibration.job_data = None;
    calibration.model_target_temp_c = None;
    calibration.thermal_plant_completion_disarm_pending = false;
    calibration.immediate_heater_disarm_pending = true;
    manual_pps.clear();
    *memory_commit_due_ms = None;
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn mark_eeprom_required(
    ui_state: &mut FrontPanelUiState,
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
    memory_commit_due_ms: &mut Option<u64>,
    fault: Option<PersistenceFault>,
) {
    let data_incompatible = ui_state.eeprom_data_incompatible;
    let has_commit_fault = fault.is_some();
    begin_mutating_eeprom_maintenance(ui_state, calibration, manual_pps, memory_commit_due_ms);
    if has_commit_fault {
        ui_state.eeprom_data_incompatible = data_incompatible;
    }
    ui_state.eeprom_required = true;
    ui_state.heater_lock_reason = Some(HeaterLockReason::PersistenceRequired);
    ui_state.persistence_fault = fault.or_else(|| ui_state.persistence_fault.clone());
    ui_state.persistence_fault_attention_pending = true;
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn eeprom_storage_failure_response(response: &UsbFrame) -> bool {
    let UsbFrame::Response {
        ok: false,
        error: Some(error),
        ..
    } = response
    else {
        return false;
    };
    matches!(
        error.code.as_str(),
        "eeprom_unavailable"
            | "eeprom_read_failed"
            | "memory_commit_write_failed"
            | "memory_commit_write_address_nack"
            | "memory_commit_write_data_nack"
            | "memory_commit_write_unknown_nack"
            | "memory_commit_write_bus_error"
            | "memory_commit_write_arbitration_lost"
            | "memory_commit_write_other_error"
            | "memory_commit_verify_unreadable"
            | "memory_commit_verify_mismatch"
            | "safety_calibration_persistence_failed"
            | "thermal_policy_persistence_failed"
            | "user_preferences_persistence_failed"
            | "network_pairing_persistence_failed"
            | "layout_marker_persistence_failed"
            | "thermal_plant_persistence_failed"
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn apply_successful_eeprom_maintenance_operation(
    op: EepromMaintenanceOp,
    ui_state: &mut FrontPanelUiState,
    memory_config: &mut MemoryConfig,
    memory_commit_due_ms: &mut Option<u64>,
) {
    if matches!(op, EepromMaintenanceOp::Erase) {
        *memory_config = MemoryConfig::default();
        *memory_commit_due_ms = None;
        apply_memory_config_to_ui(ui_state, memory_config);
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn discard_deferred_memory_commit_for_incompatible_eeprom(
    eeprom_data_incompatible: bool,
    memory_commit_due_ms: &mut Option<u64>,
) {
    if eeprom_data_incompatible {
        *memory_commit_due_ms = None;
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn usb_eeprom_maintenance_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    command: EepromMaintenanceCommand,
    i2c: &mut I2c<'_>,
    _elapsed_ms: u64,
) -> UsbFrame {
    match command.op {
        EepromMaintenanceOp::Read => {
            let (Some(offset), Some(length)) = (command.offset, command.length) else {
                return usb_error_response(
                    request_id,
                    "eeprom_range_required",
                    "EEPROM read requires offset and length.",
                );
            };
            let length = usize::from(length);
            if length == 0
                || length > flux_purr_firmware::control_plane::EEPROM_MAINTENANCE_CHUNK_MAX
                || usize::from(offset) + length > usize::from(M24C64_CAPACITY_BYTES)
            {
                return usb_error_response(
                    request_id,
                    "eeprom_range_invalid",
                    "EEPROM read range is invalid.",
                );
            }
            let Some(address) = probe_eeprom_address(i2c).await else {
                return usb_error_response(
                    request_id,
                    "eeprom_unavailable",
                    "EEPROM is unavailable.",
                );
            };
            let mut bytes = heapless::Vec::new();
            let _ = bytes.resize_default(length);
            if read_eeprom_bytes_chunked(i2c, address, offset, bytes.as_mut_slice())
                .await
                .is_err()
            {
                return usb_error_response(request_id, "eeprom_read_failed", "EEPROM read failed.");
            }
            usb_response(request_id, UsbResponsePayload::EepromBytes(bytes))
        }
        EepromMaintenanceOp::Write => {
            let (Some(offset), Some(bytes)) = (command.offset, command.bytes) else {
                return usb_error_response(
                    request_id,
                    "eeprom_write_required",
                    "EEPROM write requires offset and bytes.",
                );
            };
            if bytes.is_empty()
                || usize::from(offset) + bytes.len() > usize::from(M24C64_CAPACITY_BYTES)
            {
                return usb_error_response(
                    request_id,
                    "eeprom_range_invalid",
                    "EEPROM write range is invalid.",
                );
            }
            match write_eeprom_bytes_verified(i2c, offset, bytes.as_slice()).await {
                Ok(()) => usb_response(request_id, UsbResponsePayload::Ack),
                Err(error) => usb_error_response(request_id, error.code(), error.message()),
            }
        }
        EepromMaintenanceOp::Erase => {
            let erased = [0xff; flux_purr_firmware::control_plane::EEPROM_MAINTENANCE_CHUNK_MAX];
            let mut offset = 0u16;
            while offset < M24C64_CAPACITY_BYTES {
                if let Err(error) = write_eeprom_bytes_verified(i2c, offset, &erased).await {
                    return usb_error_response(request_id, error.code(), error.message());
                }
                offset = offset.saturating_add(erased.len() as u16);
            }
            usb_response(request_id, UsbResponsePayload::Ack)
        }
    }
}

#[cfg(test)]
#[inline(never)]
pub(crate) fn memory_record_length_from_header(header: &[u8], slot_size: usize) -> Option<usize> {
    if header.len() < MEMORY_RECORD_HEADER_LEN
        || header[0..4] != *b"FPM1"
        || header[4] != MEMORY_RECORD_FORMAT_VERSION
        || usize::from(header[5]) != MEMORY_RECORD_HEADER_LEN
    {
        return None;
    }
    let payload_len = usize::from(u16::from_le_bytes([header[6], header[7]]));
    let record_len = MEMORY_RECORD_HEADER_LEN.checked_add(payload_len)?;
    (record_len <= slot_size).then_some(record_len)
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn read_eeprom_persist_record(
    i2c: &mut I2c<'_>,
    address: u8,
    offset: u16,
    slot_size: usize,
    staging: &mut [u8],
) -> Result<Option<PersistRecord>, ()> {
    if slot_size < FPR2_HEADER_LEN || staging.len() < FPR2_HEADER_LEN {
        return Ok(None);
    }
    read_eeprom_bytes_chunked(i2c, address, offset, &mut staging[..FPR2_HEADER_LEN]).await?;
    if staging[..4] != *b"FPR2" {
        return Ok(None);
    }
    let payload_len = usize::from(u16::from_le_bytes([staging[12], staging[13]]));
    let Some(record_len) = FPR2_HEADER_LEN.checked_add(payload_len) else {
        return Ok(None);
    };
    if record_len > slot_size || record_len > staging.len() {
        return Ok(None);
    }
    read_eeprom_bytes_chunked(
        i2c,
        address,
        offset.saturating_add(FPR2_HEADER_LEN as u16),
        &mut staging[FPR2_HEADER_LEN..record_len],
    )
    .await?;
    Ok(decode_persist_record(&staging[..record_len]).ok())
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn load_eeprom_memory_record(
    i2c: &mut I2c<'_>,
    scratch: &mut MemoryIoScratch,
    record_staging: &mut [u8; EEPROM_RECORD_STAGING_BYTES],
) -> (Option<MemoryRecord>, bool, bool, bool) {
    let Some(address) = probe_eeprom_address(i2c).await else {
        info!("memory restore skipped: eeprom unavailable");
        return (None, false, true, false);
    };

    let mut marker_scan = scan_fpr2_layout_markers(
        i2c,
        address,
        &mut SensitiveEepromStaging::new(&mut record_staging[..FPR2_MAX_RECORD_SIZE]),
    )
    .await;

    // A valid ACTIVE marker makes the new format authoritative. Without one,
    // keep reading legacy data so an interrupted migration can fall back
    // safely instead of treating PREPARED records as production state.
    if marker_scan.latest_active_marker.is_none() {
        scan_legacy_slots(i2c, address, scratch, &mut marker_scan).await;
    }

    let active_generation = marker_scan
        .latest_active_marker
        .map(|marker| marker.generation);
    let read_generation = active_generation.or_else(|| {
        marker_scan
            .latest_prepared_marker
            .filter(|marker| marker.kind == LayoutMarkerKind::LegacyMigration)
            .map(|marker| marker.generation)
    });
    let mut staging = SensitiveEepromStaging::new(&mut record_staging[..FPR2_MAX_RECORD_SIZE]);
    let (domains, domain_contains_data, domain_read_failed) =
        read_fpr2_domains(i2c, address, &mut staging, read_generation).await;
    marker_scan.contains_data |= domain_contains_data;
    marker_scan.read_failed |= domain_read_failed;
    let domains = domains;

    let prepared_recovery = marker_scan.latest_active_marker.is_none()
        && marker_scan.latest_prepared_marker.is_some_and(|marker| {
            marker.kind == LayoutMarkerKind::LegacyMigration
                && !marker_scan.legacy_format_present
                && fpr2_prepared_generation_is_complete(&domains, marker.generation)
        });
    let selected = if let Some(generation) = active_generation {
        merge_persist_records(&domains, generation)
    } else if prepared_recovery {
        merge_persist_records(
            &domains,
            marker_scan
                .latest_prepared_marker
                .map_or(0, |marker| marker.generation),
        )
    } else {
        None
    };
    let current_format_valid = selected.is_some();

    if let Some(record) = &selected {
        info!(
            "memory restore ok seq={=u32} target_c={=i16} slot={=u8} active_cooling={=bool} wifi_ssid_len={=u8} telemetry_ms={=u32}",
            record.sequence,
            record.config.target_temp_c,
            record.config.selected_preset_slot as u8,
            record.config.active_cooling_enabled,
            record.config.wifi_ssid.len() as u8,
            record.config.telemetry_interval_ms,
        );
    } else {
        info!("memory restore unavailable -> using defaults");
    }

    let incompatible = marker_scan.legacy_format_present
        || eeprom_data_is_incompatible(current_format_valid, marker_scan.contains_data);
    let required = (marker_scan.read_failed && selected.is_none())
        || (marker_scan.contains_data && (domains[0].is_none() || domains[1].is_none()))
        || (marker_scan.contains_data
            && marker_scan.latest_active_marker.is_none()
            && !prepared_recovery);
    (selected, incompatible, required, prepared_recovery)
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct EepromMarkerScan {
    contains_data: bool,
    legacy_format_present: bool,
    read_failed: bool,
    latest_active_marker: Option<LayoutMarker>,
    latest_prepared_marker: Option<LayoutMarker>,
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn scan_fpr2_layout_markers(
    i2c: &mut I2c<'_>,
    address: u8,
    staging: &mut SensitiveEepromStaging<'_>,
) -> EepromMarkerScan {
    let mut scan = EepromMarkerScan {
        contains_data: false,
        legacy_format_present: false,
        read_failed: false,
        latest_active_marker: None,
        latest_prepared_marker: None,
    };
    for offset in [FPR2_LAYOUT_A_OFFSET, FPR2_LAYOUT_B_OFFSET] {
        staging.bytes.fill(0xff);
        let candidate =
            read_eeprom_persist_record(i2c, address, offset, FPR2_LAYOUT_SLOT_SIZE, staging.bytes)
                .await;
        scan.contains_data |= eeprom_bytes_contain_data(&staging.bytes[..FPR2_HEADER_LEN]);
        let Some(candidate) = (match candidate {
            Ok(candidate) => candidate,
            Err(()) => {
                scan.read_failed = true;
                None
            }
        }) else {
            continue;
        };
        let PersistDomainData::LayoutMarker(marker) = candidate.data else {
            continue;
        };
        if marker.generation != candidate.sequence {
            continue;
        }
        match marker.status {
            LayoutMarkerStatus::Active
                if scan
                    .latest_active_marker
                    .is_none_or(|current| candidate.sequence > current.generation) =>
            {
                scan.latest_active_marker = Some(marker);
            }
            LayoutMarkerStatus::Prepared
                if scan
                    .latest_prepared_marker
                    .is_none_or(|current| candidate.sequence > current.generation) =>
            {
                scan.latest_prepared_marker = Some(marker);
            }
            _ => {}
        }
    }
    scan
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn scan_legacy_slots(
    i2c: &mut I2c<'_>,
    address: u8,
    scratch: &mut MemoryIoScratch,
    scan: &mut EepromMarkerScan,
) {
    for offset in [
        PREVIOUS_MEMORY_SLOT_A_OFFSET,
        PREVIOUS_MEMORY_SLOT_B_OFFSET,
        LEGACY_MEMORY_SLOT_A_OFFSET,
        LEGACY_MEMORY_SLOT_B_OFFSET,
        MEMORY_SLOT_A_OFFSET,
        MEMORY_SLOT_B_OFFSET,
    ] {
        let probe_len = 4;
        match read_eeprom_bytes_chunked(i2c, address, offset, &mut scratch.bytes[..probe_len]).await
        {
            Ok(()) => {
                scan.contains_data |= eeprom_bytes_contain_data(&scratch.bytes[..probe_len]);
                scan.legacy_format_present |= scratch.bytes[..4] == *b"FPM1";
            }
            Err(_) => scan.read_failed = true,
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn read_fpr2_domains(
    i2c: &mut I2c<'_>,
    address: u8,
    staging: &mut SensitiveEepromStaging<'_>,
    read_generation: Option<u32>,
) -> ([Option<PersistRecord>; 6], bool, bool) {
    let mut domains: [Option<PersistRecord>; 6] = [None, None, None, None, None, None];
    let mut contains_data = false;
    let mut read_failed = false;
    for (domain, offsets, slot_size) in [
        (
            PersistDomain::SafetyCalibration,
            [FPR2_SAFETY_A_OFFSET, FPR2_SAFETY_B_OFFSET],
            FPR2_SAFETY_SLOT_SIZE,
        ),
        (
            PersistDomain::ThermalPolicy,
            [FPR2_THERMAL_A_OFFSET, FPR2_THERMAL_B_OFFSET],
            FPR2_THERMAL_SLOT_SIZE,
        ),
        (
            PersistDomain::UserPreferences,
            [FPR2_PREFERENCES_OFFSET, 0],
            128,
        ),
        (
            PersistDomain::NetworkAndPairing,
            [FPR2_NETWORK_OFFSET, 0],
            256,
        ),
        (
            PersistDomain::ThermalPlant,
            [FPR2_THERMAL_PLANT_OFFSET, 0],
            FPR2_THERMAL_SLOT_SIZE,
        ),
    ] {
        let Some(read_generation) = read_generation else {
            break;
        };
        let mut selected = None;
        for offset in offsets
            .iter()
            .copied()
            .take(usize::from(domain.slot_count()))
        {
            staging.bytes.fill(0xff);
            let candidate =
                read_eeprom_persist_record(i2c, address, offset, slot_size, staging.bytes).await;
            contains_data |= eeprom_bytes_contain_data(&staging.bytes[..FPR2_HEADER_LEN]);
            let candidate = match candidate {
                Ok(candidate) => candidate,
                Err(()) => {
                    read_failed = true;
                    None
                }
            };
            if let Some(candidate) = candidate
                && candidate.data.domain() == domain
                && candidate.sequence <= read_generation
                && selected
                    .as_ref()
                    .is_none_or(|current: &PersistRecord| candidate.sequence > current.sequence)
            {
                selected = Some(candidate);
            }
        }
        domains[domain as usize - 1] = selected;
    }
    (domains, contains_data, read_failed)
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn merge_persist_records(
    domains: &[Option<PersistRecord>; 6],
    sequence: u32,
) -> Option<MemoryRecord> {
    if !fpr2_snapshot_is_complete(domains, sequence) {
        return None;
    }
    let mut config = flux_purr_firmware::memory::MemoryConfig::default();
    let mut present = false;
    for record in domains.iter().flatten() {
        present = true;
        match &record.data {
            PersistDomainData::SafetyCalibration(value) => value.apply_to_config(&mut config),
            PersistDomainData::ThermalPolicy(value) => value.apply_to_config(&mut config),
            PersistDomainData::UserPreferences(value) => value.apply_to_config(&mut config),
            PersistDomainData::NetworkAndPairing(value) => value.apply_to_config(&mut config),
            PersistDomainData::LayoutMarker(_) => {}
            PersistDomainData::ThermalPlant(value) => value.apply_to_config(&mut config),
        }
    }
    config.sanitize();
    present.then_some(MemoryRecord { sequence, config })
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn load_legacy_eeprom_memory_record(
    i2c: &mut I2c<'_>,
    scratch: &mut MemoryIoScratch,
    record_staging: &mut [u8; EEPROM_RECORD_STAGING_BYTES],
) -> (Option<MemoryRecord>, bool) {
    let Some(address) = probe_eeprom_address(i2c).await else {
        return (None, true);
    };
    let mut selected: Option<MemoryRecord> = None;
    let mut read_failed = false;
    for (offset, length) in [
        (MEMORY_SLOT_A_OFFSET, MEMORY_SLOT_SIZE),
        (MEMORY_SLOT_B_OFFSET, MEMORY_SLOT_SIZE),
        (PREVIOUS_MEMORY_SLOT_A_OFFSET, PREVIOUS_MEMORY_SLOT_SIZE),
        (PREVIOUS_MEMORY_SLOT_B_OFFSET, PREVIOUS_MEMORY_SLOT_SIZE),
        (LEGACY_MEMORY_SLOT_A_OFFSET, LEGACY_MEMORY_SLOT_SIZE),
        (LEGACY_MEMORY_SLOT_B_OFFSET, LEGACY_MEMORY_SLOT_SIZE),
    ] {
        let candidate = read_legacy_record_stream(
            i2c,
            LegacyRecordReadInput {
                address,
                offset,
                slot_size: length,
                scratch,
                record_staging,
            },
        )
        .await
        .ok();
        if candidate.is_none() {
            read_failed = true;
        }
        selected = match (selected, candidate) {
            (Some(current), Some(candidate)) if candidate.sequence > current.sequence => {
                Some(candidate)
            }
            (Some(current), _) => Some(current),
            (None, candidate) => candidate,
        };
        EmbassyTimer::after_millis(0).await;
    }
    (selected, read_failed)
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct LegacyRecordReadInput<'a> {
    address: u8,
    offset: u16,
    slot_size: usize,
    scratch: &'a mut MemoryIoScratch,
    record_staging: &'a mut [u8; EEPROM_RECORD_STAGING_BYTES],
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn read_legacy_record_stream(
    i2c: &mut I2c<'_>,
    input: LegacyRecordReadInput<'_>,
) -> Result<MemoryRecord, ()> {
    let LegacyRecordReadInput {
        address,
        offset,
        slot_size,
        scratch,
        record_staging,
    } = input;
    let mut header = [0u8; MEMORY_RECORD_HEADER_LEN];
    read_eeprom_bytes_chunked(i2c, address, offset, &mut header).await?;
    let Some((payload_len, wide_tlv_lengths)) = legacy_record_shape(&header, slot_size) else {
        return Err(());
    };
    let mut config = MemoryConfig {
        commissioning_required: false,
        ..MemoryConfig::default()
    };
    let mut crc = persistence_crc32_update(0xffff_ffff, &header[..12]);
    let mut payload_cursor = 0usize;
    let staging = SensitiveEepromStaging::new(record_staging);
    while payload_cursor < payload_len {
        let header_len = if wide_tlv_lengths { 3 } else { 2 };
        if payload_len - payload_cursor < header_len {
            return Err(());
        }
        let tlv_offset = offset
            .checked_add(MEMORY_RECORD_HEADER_LEN as u16)
            .and_then(|base| base.checked_add(payload_cursor as u16))
            .ok_or(())?;
        read_eeprom_bytes_chunked(i2c, address, tlv_offset, &mut scratch.bytes[..header_len])
            .await?;
        crc = persistence_crc32_update(crc, &scratch.bytes[..header_len]);
        let tag = scratch.bytes[0];
        let value_len = if wide_tlv_lengths {
            usize::from(u16::from_le_bytes([scratch.bytes[1], scratch.bytes[2]]))
        } else {
            usize::from(scratch.bytes[1])
        };
        payload_cursor = payload_cursor.checked_add(header_len).ok_or(())?;
        if value_len > payload_len - payload_cursor {
            return Err(());
        }
        let mut value_read = 0usize;
        // The transient active record is part of the legacy configuration
        // and must survive FPM1 -> FPR2 migration. The older raw plant
        // records remain inspection-only and are intentionally not staged.
        let collect = !matches!(tag, 0x36 | 0x37);
        while value_read < value_len {
            let chunk_len = (value_len - value_read).min(EEPROM_WRITE_CHUNK_MAX_BYTES);
            let value_offset = offset
                .checked_add(MEMORY_RECORD_HEADER_LEN as u16)
                .and_then(|base| base.checked_add(payload_cursor as u16))
                .and_then(|base| base.checked_add(value_read as u16))
                .ok_or(())?;
            read_eeprom_bytes_chunked(i2c, address, value_offset, &mut scratch.bytes[..chunk_len])
                .await?;
            crc = persistence_crc32_update(crc, &scratch.bytes[..chunk_len]);
            if collect && value_read + chunk_len <= staging.bytes.len() {
                staging.bytes[value_read..value_read + chunk_len]
                    .copy_from_slice(&scratch.bytes[..chunk_len]);
            }
            value_read += chunk_len;
        }
        if collect && value_len <= staging.bytes.len() {
            apply_legacy_config_tlv(
                &mut config,
                tag,
                &staging.bytes[..value_len],
                wide_tlv_lengths,
            )
            .map_err(|_| ())?;
        }
        payload_cursor = payload_cursor.checked_add(value_len).ok_or(())?;
    }
    let expected_crc = u32::from_le_bytes(header[12..16].try_into().map_err(|_| ())?);
    if expected_crc != (crc ^ 0xffff_ffff) {
        return Err(());
    }
    config.sanitize();
    Ok(MemoryRecord {
        sequence: u32::from_le_bytes(header[8..12].try_into().map_err(|_| ())?),
        config,
    })
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn legacy_record_shape(
    header: &[u8; MEMORY_RECORD_HEADER_LEN],
    slot_size: usize,
) -> Option<(usize, bool)> {
    if header[..4] != *b"FPM1"
        || !matches!(header[4], 1 | 2 | 3 | 4 | MEMORY_RECORD_FORMAT_VERSION)
        || usize::from(header[5]) != MEMORY_RECORD_HEADER_LEN
    {
        return None;
    }
    let payload_len = usize::from(u16::from_le_bytes([header[6], header[7]]));
    let record_len = MEMORY_RECORD_HEADER_LEN.checked_add(payload_len)?;
    (record_len <= slot_size).then_some((payload_len, header[4] >= 3))
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryCommitError {
    EncodeFailed,
    WriteFailed,
    WriteAddressNoAck,
    WriteDataNoAck,
    WriteUnknownNoAck,
    WriteBus,
    WriteArbitration,
    WriteOther,
    VerifyUnreadable,
    VerifyMismatch,
    #[cfg(feature = "hil-eeprom-commit-fault")]
    Injected,
}

#[cfg(target_arch = "xtensa")]
impl MemoryCommitError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::EncodeFailed => "memory_commit_encode_failed",
            Self::WriteFailed => "memory_commit_write_failed",
            Self::WriteAddressNoAck => "memory_commit_write_address_nack",
            Self::WriteDataNoAck => "memory_commit_write_data_nack",
            Self::WriteUnknownNoAck => "memory_commit_write_unknown_nack",
            Self::WriteBus => "memory_commit_write_bus_error",
            Self::WriteArbitration => "memory_commit_write_arbitration_lost",
            Self::WriteOther => "memory_commit_write_other_error",
            Self::VerifyUnreadable => "memory_commit_verify_unreadable",
            Self::VerifyMismatch => "memory_commit_verify_mismatch",
            #[cfg(feature = "hil-eeprom-commit-fault")]
            Self::Injected => "memory_commit_hil_injected_failure",
        }
    }

    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::EncodeFailed => "Memory record could not be encoded.",
            Self::WriteFailed => "Memory record could not be written to EEPROM.",
            Self::WriteAddressNoAck => "EEPROM did not acknowledge its I2C address.",
            Self::WriteDataNoAck => "EEPROM rejected the I2C write payload.",
            Self::WriteUnknownNoAck => "EEPROM write failed with an I2C NACK.",
            Self::WriteBus => "EEPROM write failed with an I2C bus error.",
            Self::WriteArbitration => "EEPROM write lost I2C bus arbitration.",
            Self::WriteOther => "EEPROM write failed with an uncategorized I2C error.",
            Self::VerifyUnreadable => "Memory record could not be read back after write.",
            Self::VerifyMismatch => "Memory record readback did not match the requested config.",
            #[cfg(feature = "hil-eeprom-commit-fault")]
            Self::Injected => "HIL injected EEPROM commit failure before any write.",
        }
    }
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemoryCommitFailure {
    pub(crate) error: MemoryCommitError,
    pub(crate) phase: &'static str,
    pub(crate) attempt: u8,
    pub(crate) sequence: u32,
    pub(crate) domain: PersistDomain,
    pub(crate) slot: PersistSlot,
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PersistDomainMask(u8);

#[cfg(target_arch = "xtensa")]
impl PersistDomainMask {
    pub(crate) const SAFETY: Self = Self(1 << 0);
    const THERMAL: Self = Self(1 << 1);
    const PREFERENCES: Self = Self(1 << 2);
    const NETWORK: Self = Self(1 << 3);
    pub(crate) const THERMAL_PLANT: Self = Self(1 << 4);
    pub(crate) const ALL: Self = Self(
        Self::SAFETY.0
            | Self::THERMAL.0
            | Self::PREFERENCES.0
            | Self::NETWORK.0
            | Self::THERMAL_PLANT.0,
    );

    pub(crate) const fn includes(self, domain: PersistDomain) -> bool {
        match domain {
            PersistDomain::SafetyCalibration => self.0 & Self::SAFETY.0 != 0,
            PersistDomain::ThermalPolicy => self.0 & Self::THERMAL.0 != 0,
            PersistDomain::UserPreferences => self.0 & Self::PREFERENCES.0 != 0,
            PersistDomain::NetworkAndPairing => self.0 & Self::NETWORK.0 != 0,
            PersistDomain::LayoutMarker => false,
            PersistDomain::ThermalPlant => self.0 & Self::THERMAL_PLANT.0 != 0,
        }
    }

    const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub(crate) fn from_fault(fault: Option<&PersistenceFault>) -> Self {
        let Some(fault) = fault else {
            return Self(0);
        };
        match fault.code.as_str() {
            "safety_calibration_persistence_failed" => Self::SAFETY,
            "thermal_policy_persistence_failed" => Self::THERMAL,
            "user_preferences_persistence_failed" => Self::PREFERENCES,
            "network_pairing_persistence_failed" => Self::NETWORK,
            "layout_marker_persistence_failed" => Self::ALL,
            "thermal_plant_persistence_failed" => Self::THERMAL_PLANT,
            _ => Self(0),
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn persist_domain_mask_between(
    current: &MemoryConfig,
    persisted: &MemoryConfig,
) -> PersistDomainMask {
    let mut mask = PersistDomainMask(0);
    if current.commissioning_required != persisted.commissioning_required
        || current.adc_calibration != persisted.adc_calibration
        || current.active_heater_curve != persisted.active_heater_curve
        || current.heater_curve_raw_observations != persisted.heater_curve_raw_observations
        || current.heater_curve_transaction_id != persisted.heater_curve_transaction_id
    {
        mask.0 |= PersistDomainMask::SAFETY.0;
    }
    if current.thermal_plant_transient_active != persisted.thermal_plant_transient_active {
        mask.0 |= PersistDomainMask::THERMAL_PLANT.0;
    }
    if current.active_thermal_control_profile != persisted.active_thermal_control_profile
        || current.thermal_control_profile_pps5a != persisted.thermal_control_profile_pps5a
        || current.thermal_profile_mode != persisted.thermal_profile_mode
    {
        mask.0 |= PersistDomainMask::THERMAL.0;
    }
    if current.target_temp_c != persisted.target_temp_c
        || current.selected_preset_slot != persisted.selected_preset_slot
        || current.presets_c != persisted.presets_c
        || current.active_cooling_enabled != persisted.active_cooling_enabled
        || current.post_heat_cooling_mode != persisted.post_heat_cooling_mode
        || current.heating_fan_guard_mode != persisted.heating_fan_guard_mode
        || current.telemetry_interval_ms != persisted.telemetry_interval_ms
    {
        mask.0 |= PersistDomainMask::PREFERENCES.0;
    }
    if current.wifi_ssid != persisted.wifi_ssid
        || current.wifi_password != persisted.wifi_password
        || current.wifi_auto_reconnect != persisted.wifi_auto_reconnect
        || current.wifi_static_ipv4 != persisted.wifi_static_ipv4
        || current.lan_pairing_token != persisted.lan_pairing_token
    {
        mask.0 |= PersistDomainMask::NETWORK.0;
    }
    mask
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn copy_persisted_domains(
    persisted: &mut MemoryConfig,
    current: &MemoryConfig,
    domains: PersistDomainMask,
) {
    if domains.includes(PersistDomain::SafetyCalibration) {
        persisted.commissioning_required = current.commissioning_required;
        persisted.adc_calibration = current.adc_calibration;
        persisted.active_heater_curve = current.active_heater_curve;
        persisted.heater_curve_raw_observations = current.heater_curve_raw_observations;
        persisted.heater_curve_transaction_id = current.heater_curve_transaction_id;
    }
    if domains.includes(PersistDomain::ThermalPlant) {
        persisted.thermal_plant_transient_active = current.thermal_plant_transient_active;
    }
    if domains.includes(PersistDomain::ThermalPolicy) {
        persisted.active_thermal_control_profile = current.active_thermal_control_profile;
        persisted.thermal_control_profile_pps5a = current.thermal_control_profile_pps5a;
        persisted.thermal_profile_mode = current.thermal_profile_mode;
    }
    if domains.includes(PersistDomain::UserPreferences) {
        persisted.target_temp_c = current.target_temp_c;
        persisted.selected_preset_slot = current.selected_preset_slot;
        persisted.presets_c = current.presets_c;
        persisted.active_cooling_enabled = current.active_cooling_enabled;
        persisted.post_heat_cooling_mode = current.post_heat_cooling_mode;
        persisted.heating_fan_guard_mode = current.heating_fan_guard_mode;
        persisted.telemetry_interval_ms = current.telemetry_interval_ms;
    }
    if domains.includes(PersistDomain::NetworkAndPairing) {
        persisted.wifi_ssid = current.wifi_ssid.clone();
        persisted.wifi_password = current.wifi_password.clone();
        persisted.wifi_auto_reconnect = current.wifi_auto_reconnect;
        persisted.wifi_static_ipv4 = current.wifi_static_ipv4;
        persisted.lan_pairing_token = current.lan_pairing_token;
    }
}

#[cfg(target_arch = "xtensa")]
impl MemoryCommitFailure {
    pub(crate) const fn code(self) -> &'static str {
        match self.domain {
            PersistDomain::SafetyCalibration => "safety_calibration_persistence_failed",
            PersistDomain::ThermalPolicy => "thermal_policy_persistence_failed",
            PersistDomain::UserPreferences => "user_preferences_persistence_failed",
            PersistDomain::NetworkAndPairing => "network_pairing_persistence_failed",
            PersistDomain::LayoutMarker => "layout_marker_persistence_failed",
            PersistDomain::ThermalPlant => "thermal_plant_persistence_failed",
        }
    }

    pub(crate) const fn message(self) -> &'static str {
        self.error.message()
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) const fn memory_failure_requires_heater_lock(failure: MemoryCommitFailure) -> bool {
    matches!(
        failure.domain,
        PersistDomain::SafetyCalibration
            | PersistDomain::ThermalPolicy
            | PersistDomain::ThermalPlant
            | PersistDomain::LayoutMarker
    )
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn persistence_fault_from_commit(failure: MemoryCommitFailure) -> PersistenceFault {
    let mut phase = heapless::String::new();
    let _ = phase.push_str(failure.phase);
    let mut slot = heapless::String::new();
    let _ = slot.push_str(failure.slot.as_str());
    let mut message = heapless::String::new();
    let _ = message.push_str(failure.message());
    PersistenceFault {
        code: error_code_string(failure.code()),
        phase,
        attempt: failure.attempt,
        sequence: failure.sequence,
        slot: Some(slot),
        message,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn log_memory_commit_failure(
    sink: &mut dyn PersistenceLogSink,
    failure: MemoryCommitFailure,
    terminal: bool,
) {
    use core::fmt::Write;

    let prefix = if terminal {
        "PERSISTENCE_COMMIT_FAILED"
    } else {
        "PERSISTENCE_COMMIT_ATTEMPT_FAILED"
    };
    let mut line = heapless::String::<256>::new();
    let _ = writeln!(
        line,
        "{prefix} code={} phase={} attempt={} sequence={} slot={} message={}",
        failure.code(),
        failure.phase,
        failure.attempt,
        failure.sequence,
        failure.slot.as_str(),
        failure.message(),
    );
    sink.write_line(line.as_bytes());
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn memory_record_write_chunk_len(absolute_offset: usize, remaining: usize) -> usize {
    eeprom_maintenance_write_chunk_len(absolute_offset, remaining)
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct PersistRecordWriteInput<'a> {
    sequence: u32,
    data: &'a PersistDomainData,
    slot: PersistSlot,
    scratch: &'a mut MemoryIoScratch,
    record_staging: &'a mut [u8; EEPROM_RECORD_STAGING_BYTES],
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn write_eeprom_persist_record(
    i2c: &mut I2c<'_>,
    input: PersistRecordWriteInput<'_>,
) -> Result<(), MemoryCommitError> {
    let PersistRecordWriteInput {
        sequence,
        data,
        slot,
        scratch,
        record_staging,
    } = input;
    let staging = SensitiveEepromStaging::new(&mut record_staging[..FPR2_MAX_RECORD_SIZE]);
    staging.bytes.fill(0xff);
    let record_len = encode_persist_record(sequence, data, staging.bytes)
        .map_err(|_| MemoryCommitError::EncodeFailed)?;
    let domain = data.domain();
    let base_offset = domain.offset(slot);
    let Some(address) = probe_eeprom_address(i2c).await else {
        return Err(MemoryCommitError::WriteAddressNoAck);
    };
    let mut written = 0usize;
    while written < record_len {
        let absolute_offset = usize::from(base_offset) + written;
        let chunk_len = memory_record_write_chunk_len(absolute_offset, record_len - written);
        let chunk_offset =
            u16::try_from(absolute_offset).map_err(|_| MemoryCommitError::WriteFailed)?;
        scratch.bytes[..chunk_len].copy_from_slice(&staging.bytes[written..written + chunk_len]);
        let write_result = {
            let mut eeprom = M24c64::with_address(&mut *i2c, address);
            eeprom
                .write_page_async(chunk_offset, &scratch.bytes[..chunk_len])
                .await
        };
        write_result.map_err(memory_commit_error_from_eeprom)?;
        written += chunk_len;
        EmbassyTimer::after_millis(EEPROM_WRITE_CYCLE_DELAY_MS).await;
        EmbassyTimer::after_millis(0).await;
    }

    let mut read = 0usize;
    while read < record_len {
        let chunk_len = (record_len - read).min(EEPROM_WRITE_CHUNK_MAX_BYTES);
        let chunk_offset = base_offset
            .checked_add(read as u16)
            .ok_or(MemoryCommitError::VerifyUnreadable)?;
        let read_result = {
            let mut eeprom = M24c64::with_address(&mut *i2c, address);
            eeprom
                .read_bytes_async(chunk_offset, &mut scratch.bytes[..chunk_len])
                .await
        };
        read_result.map_err(|_| MemoryCommitError::VerifyUnreadable)?;
        if scratch.bytes[..chunk_len] != staging.bytes[read..read + chunk_len] {
            return Err(MemoryCommitError::VerifyMismatch);
        }
        read += chunk_len;
        EmbassyTimer::after_millis(0).await;
    }
    let verified = decode_persist_record(&staging.bytes[..record_len])
        .map_err(|_| MemoryCommitError::VerifyUnreadable)?;
    if verified.sequence != sequence || verified.data != *data {
        return Err(MemoryCommitError::VerifyMismatch);
    }
    Ok(())
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct LayoutMarkerWriteInput<'a> {
    generation: u32,
    status: LayoutMarkerStatus,
    kind: LayoutMarkerKind,
    slot: PersistSlot,
    phase: &'static str,
    persistence_log_sink: &'a mut dyn PersistenceLogSink,
    record_staging: &'a mut [u8; EEPROM_RECORD_STAGING_BYTES],
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn write_layout_marker_record(
    i2c: &mut I2c<'_>,
    input: LayoutMarkerWriteInput<'_>,
) -> Result<(), MemoryCommitFailure> {
    let LayoutMarkerWriteInput {
        generation,
        status,
        kind,
        slot,
        phase,
        persistence_log_sink,
        record_staging,
    } = input;
    let data = PersistDomainData::LayoutMarker(LayoutMarker {
        generation,
        status,
        kind,
    });
    let mut scratch = new_memory_io_scratch();
    let result = {
        #[cfg(feature = "hil-eeprom-commit-fault")]
        {
            Err(MemoryCommitError::Injected)
        }
        #[cfg(not(feature = "hil-eeprom-commit-fault"))]
        {
            write_eeprom_persist_record(
                i2c,
                PersistRecordWriteInput {
                    sequence: generation,
                    data: &data,
                    slot,
                    scratch: &mut scratch,
                    record_staging,
                },
            )
            .await
        }
    };
    if let Err(error) = result {
        let failure = MemoryCommitFailure {
            error,
            phase,
            attempt: 1,
            sequence: generation,
            domain: PersistDomain::LayoutMarker,
            slot,
        };
        log_memory_commit_failure(persistence_log_sink, failure, true);
        return Err(failure);
    }
    Ok(())
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct PersistMemoryDomainsInput<'a> {
    sequence: u32,
    expected_config: &'a MemoryConfig,
    domains_to_write: PersistDomainMask,
    double_slot_domains: bool,
    phase: &'static str,
    persistence_log_sink: &'a mut dyn PersistenceLogSink,
    record_staging: &'a mut [u8; EEPROM_RECORD_STAGING_BYTES],
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn persist_memory_domains(
    i2c: &mut I2c<'_>,
    input: PersistMemoryDomainsInput<'_>,
) -> Result<(), MemoryCommitFailure> {
    let PersistMemoryDomainsInput {
        sequence,
        expected_config,
        domains_to_write,
        double_slot_domains,
        phase,
        persistence_log_sink,
        record_staging,
    } = input;
    let domains = [
        (
            PersistDomainData::SafetyCalibration(SafetyCalibration::from_config(expected_config)),
            PersistSlot::A,
        ),
        (
            PersistDomainData::ThermalPolicy(ThermalPolicy::from_config(expected_config)),
            PersistSlot::A,
        ),
        (
            PersistDomainData::UserPreferences(UserPreferences::from_config(expected_config)),
            PersistSlot::Single,
        ),
        (
            PersistDomainData::NetworkAndPairing(NetworkAndPairing::from_config(expected_config)),
            PersistSlot::Single,
        ),
        (
            PersistDomainData::ThermalPlant(ThermalPlantPersistence::from_config(expected_config)),
            PersistSlot::Single,
        ),
    ];
    let mut scratch = new_memory_io_scratch();
    for (data, single_slot) in domains.iter() {
        let domain = data.domain();
        if !domains_to_write.includes(domain) || (domain.slot_count() == 2) != double_slot_domains {
            continue;
        }
        let slot = if domain.slot_count() == 2 {
            if sequence % 2 == 1 {
                PersistSlot::A
            } else {
                PersistSlot::B
            }
        } else {
            *single_slot
        };
        #[cfg(feature = "hil-eeprom-commit-fault")]
        let result = Err(MemoryCommitError::Injected);
        #[cfg(not(feature = "hil-eeprom-commit-fault"))]
        let result = write_eeprom_persist_record(
            i2c,
            PersistRecordWriteInput {
                sequence,
                data,
                slot,
                scratch: &mut scratch,
                record_staging,
            },
        )
        .await;
        if let Err(error) = result {
            let failure = MemoryCommitFailure {
                error,
                phase,
                attempt: 1,
                sequence,
                domain,
                slot,
            };
            log_memory_commit_failure(persistence_log_sink, failure, true);
            return Err(failure);
        }
    }
    Ok(())
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct CommitMemoryConfigInput<'a> {
    pub(crate) memory_sequence: &'a mut u32,
    pub(crate) memory_config: &'a MemoryConfig,
    pub(crate) domains_to_write: PersistDomainMask,
    pub(crate) persistence_log_sink: &'a mut dyn PersistenceLogSink,
    pub(crate) record_staging: &'a mut [u8; EEPROM_RECORD_STAGING_BYTES],
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn commit_memory_config_now(
    i2c: &mut I2c<'_>,
    input: CommitMemoryConfigInput<'_>,
) -> Result<(), MemoryCommitFailure> {
    let CommitMemoryConfigInput {
        memory_sequence,
        memory_config,
        domains_to_write,
        persistence_log_sink,
        record_staging,
    } = input;
    if domains_to_write.is_empty() {
        return Ok(());
    }
    let mut expected_config = memory_config.clone();
    expected_config.sanitize();
    let next_sequence = memory_sequence.saturating_add(1);
    let marker_slot = if next_sequence % 2 == 1 {
        PersistSlot::A
    } else {
        PersistSlot::B
    };
    write_layout_marker_record(
        i2c,
        LayoutMarkerWriteInput {
            generation: next_sequence,
            status: LayoutMarkerStatus::Prepared,
            kind: LayoutMarkerKind::Commit,
            slot: marker_slot,
            phase: "prepared",
            persistence_log_sink,
            record_staging,
        },
    )
    .await?;
    persist_memory_domains(
        i2c,
        PersistMemoryDomainsInput {
            sequence: next_sequence,
            expected_config: &expected_config,
            domains_to_write,
            double_slot_domains: true,
            phase: "write",
            persistence_log_sink,
            record_staging,
        },
    )
    .await?;
    persist_memory_domains(
        i2c,
        PersistMemoryDomainsInput {
            sequence: next_sequence,
            expected_config: &expected_config,
            domains_to_write,
            double_slot_domains: false,
            phase: "write-single",
            persistence_log_sink,
            record_staging,
        },
    )
    .await?;
    // ACTIVE is the publication point for the whole snapshot. Do not expose
    // this generation until both double-slot and single-slot domains have
    // completed their write/readback verification.
    write_layout_marker_record(
        i2c,
        LayoutMarkerWriteInput {
            generation: next_sequence,
            status: LayoutMarkerStatus::Active,
            kind: LayoutMarkerKind::Commit,
            slot: marker_slot,
            phase: "active",
            persistence_log_sink,
            record_staging,
        },
    )
    .await?;
    *memory_sequence = next_sequence;
    Ok(())
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct CommitMemoryDomainsWithoutMarkerInput<'a> {
    sequence: u32,
    memory_config: &'a MemoryConfig,
    domains_to_write: PersistDomainMask,
    phase: &'static str,
    persistence_log_sink: &'a mut dyn PersistenceLogSink,
    record_staging: &'a mut [u8; EEPROM_RECORD_STAGING_BYTES],
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn commit_memory_config_domains_without_marker(
    i2c: &mut I2c<'_>,
    input: CommitMemoryDomainsWithoutMarkerInput<'_>,
) -> Result<(), MemoryCommitFailure> {
    let CommitMemoryDomainsWithoutMarkerInput {
        sequence,
        memory_config,
        domains_to_write,
        phase,
        persistence_log_sink,
        record_staging,
    } = input;
    let mut expected_config = memory_config.clone();
    expected_config.sanitize();
    persist_memory_domains(
        i2c,
        PersistMemoryDomainsInput {
            sequence,
            expected_config: &expected_config,
            domains_to_write,
            double_slot_domains: true,
            phase,
            persistence_log_sink,
            record_staging,
        },
    )
    .await?;
    persist_memory_domains(
        i2c,
        PersistMemoryDomainsInput {
            sequence,
            expected_config: &expected_config,
            domains_to_write,
            double_slot_domains: false,
            phase,
            persistence_log_sink,
            record_staging,
        },
    )
    .await
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn initialize_fpr2_defaults(
    i2c: &mut I2c<'_>,
    persistence_log_sink: &mut dyn PersistenceLogSink,
    record_staging: &mut [u8; EEPROM_RECORD_STAGING_BYTES],
) -> Option<u32> {
    let mut sequence = 0;
    if commit_memory_config_now(
        i2c,
        CommitMemoryConfigInput {
            memory_sequence: &mut sequence,
            memory_config: &flux_purr_firmware::memory::MemoryConfig::default(),
            domains_to_write: PersistDomainMask::ALL,
            persistence_log_sink,
            record_staging,
        },
    )
    .await
    .is_err()
    {
        return None;
    }
    Some(sequence)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn migrate_legacy_memory_config(
    i2c: &mut I2c<'_>,
    legacy_sequence: u32,
    config: &MemoryConfig,
    persistence_log_sink: &mut dyn PersistenceLogSink,
    record_staging: &mut [u8; EEPROM_RECORD_STAGING_BYTES],
) -> Result<u32, MemoryCommitFailure> {
    let sequence = legacy_sequence.saturating_add(1);
    for slot in [PersistSlot::A, PersistSlot::B] {
        write_layout_marker_record(
            i2c,
            LayoutMarkerWriteInput {
                generation: sequence,
                status: LayoutMarkerStatus::Prepared,
                kind: LayoutMarkerKind::LegacyMigration,
                slot,
                phase: "prepared",
                persistence_log_sink,
                record_staging,
            },
        )
        .await?;
    }
    commit_memory_config_domains_without_marker(
        i2c,
        CommitMemoryDomainsWithoutMarkerInput {
            sequence,
            memory_config: config,
            domains_to_write: PersistDomainMask::ALL,
            phase: "write-migration",
            persistence_log_sink,
            record_staging,
        },
    )
    .await?;
    let mut scratch = new_memory_io_scratch();
    invalidate_legacy_v5_magic(i2c, &mut scratch)
        .await
        .map_err(|error| {
            let failure = MemoryCommitFailure {
                error,
                phase: "invalidate",
                attempt: 1,
                sequence,
                domain: PersistDomain::LayoutMarker,
                slot: PersistSlot::Single,
            };
            log_memory_commit_failure(persistence_log_sink, failure, true);
            failure
        })?;
    for slot in [PersistSlot::A, PersistSlot::B] {
        write_layout_marker_record(
            i2c,
            LayoutMarkerWriteInput {
                generation: sequence,
                status: LayoutMarkerStatus::Active,
                kind: LayoutMarkerKind::LegacyMigration,
                slot,
                phase: "active",
                persistence_log_sink,
                record_staging,
            },
        )
        .await?;
    }
    Ok(sequence)
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
pub(crate) struct PreparedFpr2DomainSpec {
    domain: PersistDomain,
    offsets: [u16; 2],
    slot_size: usize,
    sequence: u32,
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn read_prepared_fpr2_domain(
    i2c: &mut I2c<'_>,
    address: u8,
    spec: PreparedFpr2DomainSpec,
    staging: &mut SensitiveEepromStaging<'_>,
) -> (Option<PersistRecord>, bool) {
    let mut selected: Option<PersistRecord> = None;
    let mut read_failed = false;
    for offset in spec
        .offsets
        .into_iter()
        .take(usize::from(spec.domain.slot_count()))
    {
        staging.bytes.fill(0xff);
        let candidate =
            read_eeprom_persist_record(i2c, address, offset, spec.slot_size, staging.bytes).await;
        let candidate = match candidate {
            Ok(candidate) => candidate,
            Err(()) => {
                read_failed = true;
                None
            }
        };
        if let Some(candidate) = candidate
            && candidate.data.domain() == spec.domain
            && candidate.sequence == spec.sequence
            && selected
                .as_ref()
                .is_none_or(|current| candidate.sequence > current.sequence)
        {
            selected = Some(candidate);
        }
    }
    (selected, read_failed)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn read_prepared_fpr2_domains(
    i2c: &mut I2c<'_>,
    address: u8,
    sequence: u32,
    staging: &mut SensitiveEepromStaging<'_>,
) -> ([Option<PersistRecord>; 6], bool) {
    let specs = [
        PreparedFpr2DomainSpec {
            domain: PersistDomain::SafetyCalibration,
            offsets: [FPR2_SAFETY_A_OFFSET, FPR2_SAFETY_B_OFFSET],
            slot_size: FPR2_SAFETY_SLOT_SIZE,
            sequence,
        },
        PreparedFpr2DomainSpec {
            domain: PersistDomain::ThermalPolicy,
            offsets: [FPR2_THERMAL_A_OFFSET, FPR2_THERMAL_B_OFFSET],
            slot_size: FPR2_THERMAL_SLOT_SIZE,
            sequence,
        },
        PreparedFpr2DomainSpec {
            domain: PersistDomain::UserPreferences,
            offsets: [FPR2_PREFERENCES_OFFSET, 0],
            slot_size: 128,
            sequence,
        },
        PreparedFpr2DomainSpec {
            domain: PersistDomain::NetworkAndPairing,
            offsets: [FPR2_NETWORK_OFFSET, 0],
            slot_size: 256,
            sequence,
        },
        PreparedFpr2DomainSpec {
            domain: PersistDomain::ThermalPlant,
            offsets: [FPR2_THERMAL_PLANT_OFFSET, 0],
            slot_size: FPR2_THERMAL_SLOT_SIZE,
            sequence,
        },
    ];
    let mut domains = [None, None, None, None, None, None];
    let mut read_failed = false;
    for spec in specs {
        let (selected, failed) = read_prepared_fpr2_domain(i2c, address, spec, staging).await;
        domains[spec.domain as usize - 1] = selected;
        read_failed |= failed;
    }
    (domains, read_failed)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn write_recovered_active_markers(
    i2c: &mut I2c<'_>,
    sequence: u32,
    persistence_log_sink: &mut dyn PersistenceLogSink,
    record_staging: &mut [u8; EEPROM_RECORD_STAGING_BYTES],
) -> Result<(), MemoryCommitFailure> {
    let mut scratch = new_memory_io_scratch();
    let active = PersistDomainData::LayoutMarker(LayoutMarker {
        generation: sequence,
        status: LayoutMarkerStatus::Active,
        kind: LayoutMarkerKind::LegacyMigration,
    });
    for slot in [PersistSlot::A, PersistSlot::B] {
        if let Err(error) = write_eeprom_persist_record(
            i2c,
            PersistRecordWriteInput {
                sequence,
                data: &active,
                slot,
                scratch: &mut scratch,
                record_staging,
            },
        )
        .await
        {
            let failure = MemoryCommitFailure {
                error,
                phase: "active-recovery",
                attempt: 1,
                sequence,
                domain: PersistDomain::LayoutMarker,
                slot,
            };
            log_memory_commit_failure(persistence_log_sink, failure, true);
            return Err(failure);
        }
    }
    Ok(())
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn recover_prepared_fpr2_layout(
    i2c: &mut I2c<'_>,
    sequence: u32,
    persistence_log_sink: &mut dyn PersistenceLogSink,
    record_staging: &mut [u8; EEPROM_RECORD_STAGING_BYTES],
) -> Result<(), MemoryCommitFailure> {
    let Some(address) = probe_eeprom_address(i2c).await else {
        let failure = MemoryCommitFailure {
            error: MemoryCommitError::VerifyUnreadable,
            phase: "active-recovery",
            attempt: 1,
            sequence,
            domain: PersistDomain::LayoutMarker,
            slot: PersistSlot::A,
        };
        log_memory_commit_failure(persistence_log_sink, failure, true);
        return Err(failure);
    };
    let mut staging = SensitiveEepromStaging::new(&mut record_staging[..FPR2_MAX_RECORD_SIZE]);
    let (domains, read_failed) =
        read_prepared_fpr2_domains(i2c, address, sequence, &mut staging).await;
    if read_failed || !fpr2_prepared_generation_is_complete(&domains, sequence) {
        let failure = MemoryCommitFailure {
            error: if read_failed {
                MemoryCommitError::VerifyUnreadable
            } else {
                MemoryCommitError::VerifyMismatch
            },
            phase: "active-recovery",
            attempt: 1,
            sequence,
            domain: PersistDomain::LayoutMarker,
            slot: PersistSlot::A,
        };
        log_memory_commit_failure(persistence_log_sink, failure, true);
        return Err(failure);
    }
    drop(staging);
    write_recovered_active_markers(i2c, sequence, persistence_log_sink, record_staging).await
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn invalidate_legacy_v5_magic(
    i2c: &mut I2c<'_>,
    scratch: &mut MemoryIoScratch,
) -> Result<(), MemoryCommitError> {
    let Some(address) = probe_eeprom_address(i2c).await else {
        return Err(MemoryCommitError::WriteAddressNoAck);
    };
    let invalid = [0xffu8; 4];
    for offset in [MEMORY_SLOT_A_OFFSET, MEMORY_SLOT_B_OFFSET] {
        let result = {
            let mut eeprom = M24c64::with_address(&mut *i2c, address);
            scratch.bytes[..invalid.len()].copy_from_slice(&invalid);
            eeprom
                .write_page_async(offset, &scratch.bytes[..invalid.len()])
                .await
        };
        result.map_err(memory_commit_error_from_eeprom)?;
        EmbassyTimer::after_millis(EEPROM_WRITE_CYCLE_DELAY_MS).await;
        EmbassyTimer::after_millis(0).await;
    }
    Ok(())
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn apply_memory_config_to_ui(state: &mut FrontPanelUiState, config: &MemoryConfig) {
    state.set_target_temp_c(config.target_temp_c);
    state.selected_preset_slot = config.selected_preset_slot;
    state.ensure_selected_preset_slot();
    state.presets_c = config.presets_c;
    state.set_fan_settings(config.post_heat_cooling_mode, config.heating_fan_guard_mode);
}

#[cfg(test)]
pub(crate) fn restore_last_persisted_memory_config(
    memory_config: &mut MemoryConfig,
    ui_state: &mut FrontPanelUiState,
    last_persisted_memory_config: &MemoryConfig,
) {
    *memory_config = last_persisted_memory_config.clone();
    apply_memory_config_to_ui(ui_state, memory_config);
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn restore_persisted_memory_domains(
    memory_config: &mut MemoryConfig,
    ui_state: &mut FrontPanelUiState,
    last_persisted_memory_config: &MemoryConfig,
    domains: PersistDomainMask,
) {
    copy_persisted_domains(memory_config, last_persisted_memory_config, domains);
    apply_memory_config_to_ui(ui_state, memory_config);
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn memory_config_from_ui(
    state: &FrontPanelUiState,
    previous: &MemoryConfig,
) -> MemoryConfig {
    MemoryConfig {
        commissioning_required: previous.commissioning_required,
        target_temp_c: state.target_temp_c,
        selected_preset_slot: state.selected_preset_slot,
        presets_c: state.presets_c,
        active_cooling_enabled: state.active_cooling_enabled,
        post_heat_cooling_mode: state.post_heat_cooling_mode,
        heating_fan_guard_mode: state.heating_fan_guard_mode,
        wifi_ssid: previous.wifi_ssid.clone(),
        wifi_password: previous.wifi_password.clone(),
        wifi_auto_reconnect: previous.wifi_auto_reconnect,
        wifi_static_ipv4: previous.wifi_static_ipv4,
        telemetry_interval_ms: previous.telemetry_interval_ms,
        adc_calibration: previous.adc_calibration,
        active_heater_curve: previous.active_heater_curve,
        heater_curve_raw_observations: previous.heater_curve_raw_observations,
        heater_curve_transaction_id: previous.heater_curve_transaction_id,
        thermal_plant_active: previous.thermal_plant_active,
        thermal_plant_transient_active: previous.thermal_plant_transient_active,
        active_thermal_control_profile: previous.active_thermal_control_profile,
        thermal_control_profile_pps5a: previous.thermal_control_profile_pps5a,
        thermal_profile_mode: previous.thermal_profile_mode,
        lan_pairing_token: previous.lan_pairing_token,
    }
}

#[allow(dead_code)]
pub(crate) fn floor_mv_to_100mv(millivolts: u16) -> u16 {
    (millivolts / 100) * 100
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn default_estimated_heater_resistance_ohms(current_temp_c: f32) -> f32 {
    HEATER_PROFILE_R20_OHMS
        * (1.0 + HEATER_PROFILE_TEMP_COEFFICIENT_PER_C * (current_temp_c - 20.0))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn projected_heater_curve(memory_config: &MemoryConfig) -> Option<HeaterCurveConfig> {
    let mut curve = HeaterCurveConfig::default();
    curve.points[0] = Some(default_heater_curve_point(HEATER_CURVE_COLD_ANCHOR_TEMP_C));
    curve.points[1] = Some(default_heater_curve_point(HEATER_CURVE_R20_ANCHOR_TEMP_C));
    let mut observed_count = 0;
    let mut last_resistance_milliohms = curve.points[1]
        .map(|point| point.resistance_milliohms)
        .unwrap_or_default();
    for (count, observation) in (2..).zip(
        memory_config
            .heater_curve_raw_observations
            .points
            .iter()
            .flatten(),
    ) {
        let temp_c = projected_rtd_temperature_c(memory_config, observation.raw_rtd_adc_mv)?;
        if !(-50.0..=450.0).contains(&temp_c) {
            return None;
        }
        let resistance_milliohms = observation
            .resistance_milliohms
            .max(last_resistance_milliohms)
            .max(heater_curve_model_floor_milliohms(temp_c));
        last_resistance_milliohms = resistance_milliohms;
        curve.points[count] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
            temp_centi_c: round_to_i16(temp_c * 100.0),
            resistance_milliohms,
        });
        observed_count += 1;
    }
    (observed_count >= 2).then_some(curve)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn estimated_heater_resistance_ohms(
    current_temp_c: f32,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> f32 {
    let estimated = preview_heater_curve
        .and_then(|curve| heater_resistance_ohms_from_curve(curve, current_temp_c))
        .or_else(|| {
            projected_heater_curve(memory_config)
                .and_then(|curve| heater_resistance_ohms_from_curve(&curve, current_temp_c))
        })
        .or_else(|| {
            heater_resistance_ohms_from_curve(&memory_config.active_heater_curve, current_temp_c)
        })
        .unwrap_or_else(|| default_estimated_heater_resistance_ohms(current_temp_c));
    estimated.max(default_estimated_heater_resistance_ohms(current_temp_c))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn effective_pps_current_limit_ma(
    capability_max_ma: u16,
    pd_observation: Option<PdStatusObservation>,
) -> u16 {
    // `current_ma` on the FUSB302B observation is the committed PPS contract,
    // not instantaneous VBUS draw. It is authoritative after a source
    // capability refresh, whereas the capability bridge remains the safe
    // provisional ceiling before a PPS contract is ready.
    pd_observation
        .filter(|observation| {
            observation.status.pd_active
                && observation.contract.kind == ContractKind::Pps
                && observation.current_ma >= MIN_HEATER_CONTRACT_MA
        })
        .map_or(capability_max_ma, |observation| {
            capability_max_ma.min(observation.current_ma)
        })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heater_available_current_ma(current_limit_ma: u16, reserve_ma: u16) -> u16 {
    current_limit_ma.saturating_sub(reserve_ma.min(current_limit_ma))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heater_safe_max_mv_for_temp(
    current_temp_c: f32,
    effective_current_limit_ma: u16,
    source_voltage_max_mv: u16,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> u16 {
    if effective_current_limit_ma == 0 {
        return 0;
    }

    let estimated_mv =
        (estimated_heater_resistance_ohms(current_temp_c, preview_heater_curve, memory_config)
            * f32::from(effective_current_limit_ma))
        .max(0.0)
        .min(f32::from(u16::MAX)) as u16;
    floor_mv_to_100mv(estimated_mv).min(source_voltage_max_mv)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn production_pps_request_ceiling_mv(
    _current_temp_c: f32,
    _source_current_limit_ma: u16,
    _reserve_ma: u16,
    source_voltage_max_mv: u16,
    _preview_heater_curve: Option<&HeaterCurveConfig>,
    _memory_config: &MemoryConfig,
) -> u16 {
    source_voltage_max_mv
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heater_available_power_mw_for_temp(
    current_temp_c: f32,
    capability_max_mv: Option<u16>,
    capability_max_ma: Option<u16>,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> u32 {
    let source_voltage_max_mv = capability_max_mv.unwrap_or(0).min(HEATER_ADJUSTABLE_MAX_MV);
    let available_current_ma = capability_max_ma.unwrap_or(0);
    if source_voltage_max_mv == 0 || available_current_ma == 0 {
        return 0;
    }

    let resistance_ohms =
        estimated_heater_resistance_ohms(current_temp_c, preview_heater_curve, memory_config);
    if !resistance_ohms.is_finite() || resistance_ohms <= 0.0 {
        return 0;
    }
    let resistance_limited_power_mw = f32::from(source_voltage_max_mv)
        * f32::from(source_voltage_max_mv)
        / resistance_ohms
        / 1_000.0;
    let contract_power_mw =
        f32::from(source_voltage_max_mv) * f32::from(available_current_ma) / 1_000.0;
    resistance_limited_power_mw
        .min(contract_power_mw)
        .max(0.0)
        .min(u32::MAX as f32) as u32
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heater_source_request_ceiling_mv(
    safe_heater_mv: u16,
    current_request_mv: u16,
    measured_heater_mv: u32,
    source_voltage_max_mv: u16,
) -> u16 {
    let measured_heater_mv = measured_heater_mv.min(u32::from(u16::MAX)) as u16;
    if measured_heater_mv == 0 || current_request_mv <= measured_heater_mv {
        return safe_heater_mv.min(source_voltage_max_mv);
    }
    let path_drop_mv = current_request_mv
        .saturating_sub(measured_heater_mv)
        .min(HEATER_PPS_PATH_DROP_COMPENSATION_MAX_MV);
    safe_heater_mv
        .saturating_add(path_drop_mv)
        .min(source_voltage_max_mv)
}

#[cfg(test)]
pub(crate) fn has_calibrated_heater_resistance_curve(memory_config: &MemoryConfig) -> bool {
    memory_config
        .active_heater_curve
        .points
        .iter()
        .flatten()
        .count()
        >= 2
        || projected_heater_curve(memory_config).is_some()
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn has_persisted_heater_resistance_curve(memory_config: &MemoryConfig) -> bool {
    projected_heater_curve(memory_config).is_some()
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn should_use_current_limit_fixed_pwm_fallback(
    duty_percent: u8,
    was_active: bool,
    safe_max_mv: u16,
    control_floor_mv: u16,
) -> bool {
    if duty_percent == 0 {
        return false;
    }

    if was_active {
        safe_max_mv < control_floor_mv.saturating_add(HEATER_CURRENT_LIMIT_RETURN_HYSTERESIS_MV)
    } else {
        safe_max_mv < control_floor_mv
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn should_apply_current_limit_fixed_pwm_fallback(
    duty_percent: u8,
    manual_pps_active: bool,
    was_active: bool,
    safe_max_mv: u16,
    control_floor_mv: u16,
) -> bool {
    !manual_pps_active
        && should_use_current_limit_fixed_pwm_fallback(
            duty_percent,
            was_active,
            safe_max_mv,
            control_floor_mv,
        )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn effective_auto_adjustable_working_floor_mv(
    settings: ThermalControlProfileSettings,
    capability_floor_mv: u16,
    adjustable_max_mv: u16,
) -> u16 {
    settings
        .auto_adjustable_working_floor_mv
        .max(capability_floor_mv)
        .clamp(capability_floor_mv, adjustable_max_mv)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn current_limit_fixed_pwm_duty_percent(
    duty_percent: u8,
    current_temp_c: f32,
    effective_current_limit_ma: u16,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> u8 {
    let fixed_mv = HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts();
    if duty_percent == 0 || fixed_mv == 0 {
        return 0;
    }

    let safe_mv = heater_safe_max_mv_for_temp(
        current_temp_c,
        effective_current_limit_ma,
        fixed_mv,
        preview_heater_curve,
        memory_config,
    );
    let capped_percent = (u32::from(safe_mv) * 100 / u32::from(fixed_mv)).min(100) as u8;
    duty_percent.min(capped_percent)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fixed_pd_pwm_duty_percent(
    duty_percent: u8,
    current_temp_c: f32,
    fixed_mv: u16,
    negotiated_current_ma: u16,
    reserve_ma: u16,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> u8 {
    if duty_percent == 0 || fixed_mv == 0 {
        return 0;
    }

    let available_current_ma = heater_available_current_ma(negotiated_current_ma, reserve_ma);
    let safe_mv = heater_safe_max_mv_for_temp(
        current_temp_c,
        available_current_ma,
        fixed_mv,
        preview_heater_curve,
        memory_config,
    );
    let capped_percent = (u32::from(safe_mv) * 100 / u32::from(fixed_mv)).min(100) as u8;
    duty_percent.min(capped_percent)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heater_request_mv_from_power_percent(
    duty_percent: u8,
    floor_mv: u16,
    ceiling_mv: u16,
) -> u16 {
    let bounded_min_mv = floor_mv.clamp(CH224Q_ADJUSTABLE_REQUEST_MIN_MV, HEATER_ADJUSTABLE_MAX_MV);
    let bounded_max_mv = ceiling_mv
        .max(CH224Q_ADJUSTABLE_REQUEST_MIN_MV)
        .clamp(bounded_min_mv, HEATER_ADJUSTABLE_MAX_MV);
    if duty_percent == 0 {
        return bounded_min_mv;
    }

    let requested_mv = integer_sqrt_floor(
        u64::from(bounded_max_mv)
            .saturating_mul(u64::from(bounded_max_mv))
            .saturating_mul(u64::from(duty_percent.min(100)))
            / 100,
    ) as u16;

    floor_mv_to_100mv(requested_mv.clamp(bounded_min_mv, bounded_max_mv))
        .clamp(bounded_min_mv, bounded_max_mv)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heater_physical_pwm_percent(
    duty_percent: u8,
    ceiling_mv: u16,
    active_request_mv: u16,
    warmup_soft_start_percent: u8,
) -> u8 {
    if duty_percent == 0 {
        return 0;
    }
    let requested_power = u64::from(duty_percent.min(100))
        .saturating_mul(u64::from(ceiling_mv).saturating_mul(u64::from(ceiling_mv)));
    let active_request_mv = active_request_mv.max(CH224Q_ADJUSTABLE_REQUEST_MIN_MV);
    let active_power = u64::from(active_request_mv).saturating_mul(u64::from(active_request_mv));
    let power_matched_percent = (requested_power / active_power.max(1)).min(100) as u8;
    let soft_started_percent = u16::from(power_matched_percent)
        .saturating_mul(u16::from(warmup_soft_start_percent.min(100)))
        / 100;
    soft_started_percent.min(100) as u8
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn apply_warmup_soft_start(duty_percent: u8, warmup_soft_start_percent: u8) -> u8 {
    (u16::from(duty_percent.min(100)).saturating_mul(u16::from(warmup_soft_start_percent.min(100)))
        / 100) as u8
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn integer_sqrt_floor(value: u64) -> u32 {
    let mut low = 0_u64;
    let mut high = u64::from(u32::MAX);
    while low <= high {
        let mid = low + ((high - low) / 2);
        let square = mid.saturating_mul(mid);
        if square == value {
            return mid as u32;
        }
        if square < value {
            low = mid.saturating_add(1);
        } else if mid == 0 {
            break;
        } else {
            high = mid - 1;
        }
    }
    high as u32
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) fn adjustable_mode_for_request(
    request_mv: u16,
    pps_max_mv: u16,
) -> ch224q::AdjustableVoltageMode {
    if request_mv <= pps_max_mv {
        ch224q::AdjustableVoltageMode::Pps
    } else {
        ch224q::AdjustableVoltageMode::Avs
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn should_blank_heater_for_adjustable_request(
    _current_request_mv: u16,
    _next_request_mv: u16,
    mode_changed: bool,
) -> bool {
    mode_changed
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn should_restore_gate_after_adjustable_request(
    blank_heater: bool,
    gate_duty_percent: u8,
) -> bool {
    !blank_heater && gate_duty_percent > 0
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn pps_request_transition_ms(mode_changed: bool) -> u64 {
    if mode_changed {
        HEATER_PPS_LARGE_TRANSITION_MS
    } else {
        HEATER_PPS_SMALL_TRANSITION_MS
    }
}

#[cfg(test)]
pub(crate) fn clamp_ch224q_adjustable_request_mv(request_mv: u16) -> u16 {
    request_mv.max(CH224Q_ADJUSTABLE_REQUEST_MIN_MV)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heater_adjustable_request_mv(
    duty_percent: u8,
    heater_enabled: bool,
    current_request_mv: u16,
    idle_request_mv: u16,
    control_floor_mv: u16,
    safe_max_mv: u16,
) -> u16 {
    if duty_percent == 0 {
        if heater_enabled {
            current_request_mv
                .saturating_sub(HEATER_PPS_REQUEST_STEP_MV)
                .max(control_floor_mv)
        } else {
            idle_request_mv
        }
    } else {
        let desired_request_mv =
            heater_request_mv_from_power_percent(duty_percent, control_floor_mv, safe_max_mv);
        if desired_request_mv.abs_diff(current_request_mv) < HEATER_PPS_REQUEST_HYSTERESIS_MV {
            current_request_mv
        } else if desired_request_mv > current_request_mv {
            current_request_mv
                .saturating_add(HEATER_PPS_REQUEST_STEP_MV)
                .min(desired_request_mv)
        } else {
            current_request_mv
                .saturating_sub(HEATER_PPS_REQUEST_STEP_MV)
                .max(desired_request_mv)
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn select_heater_power_backend(
    capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    status: Option<Status>,
) -> HeaterPowerBackend {
    let Some(capabilities) = capabilities else {
        return HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::CapabilityReadFailed,
            fixed_request_confirmed: true,
            fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
            terminal_fixed_pd_disarmed: false,
        };
    };

    select_heater_power_backend_with_capability_state(
        capabilities,
        status,
        ManualPpsState::from_capabilities(Some(capabilities)),
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn select_heater_power_backend_with_capability_state(
    capabilities: ch224q::AdjustablePowerCapabilities,
    status: Option<Status>,
    capability_state: ManualPpsState,
) -> HeaterPowerBackend {
    select_heater_power_backend_with_source_limits(
        capabilities,
        status,
        capability_state.thermal_plant_source_limits(),
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn select_heater_power_backend_with_source_limits(
    capabilities: ch224q::AdjustablePowerCapabilities,
    status: Option<Status>,
    source_limits: Option<(u16, u16, u16)>,
) -> HeaterPowerBackend {
    let Some((pps_min_mv, pps_max_mv, capability_max_ma)) = source_limits else {
        return HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::NoPps20vCapability,
            fixed_request_confirmed: true,
            fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
            terminal_fixed_pd_disarmed: false,
        };
    };
    let idle_request_mv = HEATER_ADJUSTABLE_MIN_MV.clamp(pps_min_mv, pps_max_mv);
    let avs_max_mv = if status.is_some_and(|status| status.avs_exist) {
        capabilities
            .avs_min_mv
            .zip(capabilities.avs_max_mv)
            .and_then(|(avs_min_mv, avs_max_mv)| {
                let bounded_avs_max_mv =
                    avs_max_mv.min(HEATER_ADJUSTABLE_MAX_MV.min(ch224q::CH224Q_AVS_MAX_MV));
                let first_avs_request_mv = pps_max_mv.saturating_add(100);
                if avs_min_mv <= first_avs_request_mv && bounded_avs_max_mv > pps_max_mv {
                    Some(bounded_avs_max_mv)
                } else {
                    None
                }
            })
    } else {
        None
    };
    let adjustable_max_mv = avs_max_mv.unwrap_or_else(|| pps_max_mv.min(HEATER_ADJUSTABLE_MAX_MV));
    HeaterPowerBackend::PpsMos {
        pps_min_mv,
        idle_request_mv,
        pps_max_mv,
        adjustable_max_mv,
        capability_max_ma,
        current_mode: None,
        current_request_mv: idle_request_mv,
        settle_until_ms: None,
        next_request_at_ms: 0,
        current_limit_fixed_pwm_active: false,
        current_limit_fixed_request_confirmed: false,
        terminal_fixed_pd_disarmed: false,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn constrain_heater_backend_to_controller(
    controller: ControllerKind,
    backend: HeaterPowerBackend,
) -> HeaterPowerBackend {
    match (controller, backend) {
        (
            ControllerKind::Fusb302b,
            HeaterPowerBackend::PpsMos {
                pps_min_mv,
                idle_request_mv,
                pps_max_mv,
                adjustable_max_mv,
                capability_max_ma,
                current_request_mv,
                settle_until_ms,
                next_request_at_ms,
                current_limit_fixed_pwm_active,
                current_limit_fixed_request_confirmed,
                terminal_fixed_pd_disarmed,
                ..
            },
        ) if pps_min_mv <= FUSB302B_PPS_MAX_MV => {
            let pps_max_mv = pps_max_mv.min(FUSB302B_PPS_MAX_MV);
            HeaterPowerBackend::PpsMos {
                pps_min_mv,
                idle_request_mv: idle_request_mv.clamp(pps_min_mv, pps_max_mv),
                pps_max_mv,
                adjustable_max_mv: adjustable_max_mv.min(pps_max_mv),
                capability_max_ma,
                current_mode: Some(ch224q::AdjustableVoltageMode::Pps),
                current_request_mv: current_request_mv.clamp(pps_min_mv, pps_max_mv),
                settle_until_ms,
                next_request_at_ms,
                current_limit_fixed_pwm_active,
                current_limit_fixed_request_confirmed,
                terminal_fixed_pd_disarmed,
            }
        }
        (
            ControllerKind::Fusb302b,
            HeaterPowerBackend::PpsMos {
                terminal_fixed_pd_disarmed,
                ..
            },
        ) => HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::NoPps20vCapability,
            fixed_request_confirmed: false,
            fixed_request: ch224q::VoltageRequest::V12,
            terminal_fixed_pd_disarmed,
        },
        (
            ControllerKind::Fusb302b,
            HeaterPowerBackend::FixedPdPwmFallback {
                reason,
                terminal_fixed_pd_disarmed,
                ..
            },
        ) => HeaterPowerBackend::FixedPdPwmFallback {
            reason,
            fixed_request_confirmed: false,
            fixed_request: ch224q::VoltageRequest::V12,
            terminal_fixed_pd_disarmed,
        },
        (_, backend) => backend,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn select_fusb302b_heater_power_backend(
    capabilities: Option<ch224q::AdjustablePowerCapabilities>,
) -> HeaterPowerBackend {
    let Some(capabilities) = capabilities else {
        return HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::CapabilityReadFailed,
            fixed_request_confirmed: false,
            fixed_request: ch224q::VoltageRequest::V12,
            terminal_fixed_pd_disarmed: false,
        };
    };

    constrain_heater_backend_to_controller(
        ControllerKind::Fusb302b,
        select_heater_power_backend_with_source_limits(
            capabilities,
            None,
            ManualPpsState::from_fusb302b_capabilities(Some(capabilities)).heater_source_limits(),
        ),
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn refresh_fusb302b_heater_power_backend(
    previous: HeaterPowerBackend,
    capabilities: Option<ch224q::AdjustablePowerCapabilities>,
) -> HeaterPowerBackend {
    let mut refreshed = select_fusb302b_heater_power_backend(capabilities);
    refreshed.set_terminal_fixed_pd_disarmed(previous.terminal_fixed_pd_disarmed());
    refreshed
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn apply_heater_duty<PWM>(
    heater_pwm: &mut PWM,
    duty_percent: u8,
    last_duty_percent: &mut u8,
) where
    PWM: SetDutyCycle,
{
    let effective_duty_percent = if PD_HEATER_PERMIT.load(Ordering::Acquire) != 0 {
        duty_percent
    } else {
        0
    };
    if effective_duty_percent == *last_duty_percent {
        return;
    }

    let _ = heater_pwm.set_duty_cycle_percent(effective_duty_percent);
    info!(
        "heater output -> duty={=u8}% prev={=u8}%",
        effective_duty_percent, *last_duty_percent,
    );
    *last_duty_percent = effective_duty_percent;
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct ThermalPlantDisarmContext<'a, PWM> {
    pub(crate) calibration_runtime_state: &'a mut CalibrationRuntimeState,
    pub(crate) backend: &'a mut HeaterPowerBackend,
    pub(crate) manual_pps: &'a mut ManualPpsState,
    pub(crate) pd_port: &'a PdPort,
    pub(crate) heater_pwm: &'a mut PWM,
    pub(crate) hold_pps_governor: &'a mut HoldPpsGovernor,
    pub(crate) ui_state: &'a mut FrontPanelUiState,
    pub(crate) last_heater_duty: &'a mut u8,
    pub(crate) measured_vin_mv: u32,
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn disarm_pending_thermal_plant_output<PWM>(
    context: ThermalPlantDisarmContext<'_, PWM>,
) -> bool
where
    PWM: SetDutyCycle,
{
    let ThermalPlantDisarmContext {
        calibration_runtime_state,
        backend,
        manual_pps,
        pd_port,
        heater_pwm,
        hold_pps_governor,
        ui_state,
        last_heater_duty,
        measured_vin_mv,
    } = context;
    if !latch_terminal_fixed_pd_disarm(calibration_runtime_state, backend) {
        return false;
    }

    apply_heater_duty(heater_pwm, 0, last_heater_duty);
    hold_pps_governor.reset();
    ui_state.heater_enabled = false;
    ui_state.heater_output_percent = 0;

    if !matches!(
        pd_port.restore_automatic_idle_contract(),
        PdContractRequestState::Confirmed
    ) {
        // Keep both the disarm latch and the PPS backend lock until the
        // independent PD task restores its automatic idle contract.
        return true;
    }

    if !terminal_idle_voltage_confirmed(measured_vin_mv) {
        // A source may acknowledge the request before VBUS reaches the idle
        // voltage. Keep the terminal lock active until VIN confirms it.
        return true;
    }

    calibration_runtime_state.immediate_heater_disarm_pending = false;
    let _ = manual_pps.consume_automatic_restore_pending();
    true
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn terminal_idle_voltage_confirmed(measured_vin_mv: u32) -> bool {
    measured_vin_mv.abs_diff(u32::from(FUSB302B_INITIAL_PPS_REQUEST_MV)) <= 1_000
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn pd_observation_confirms_fixed_contract(
    observation: Option<PdStatusObservation>,
    requested_mv: u16,
) -> bool {
    observation.is_some_and(|observation| {
        observation.status.pd_active
            && observation.contract.kind == ContractKind::Fixed
            && observation.contract.voltage_mv == requested_mv
    })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn latch_terminal_fixed_pd_disarm(
    calibration_runtime_state: &CalibrationRuntimeState,
    backend: &mut HeaterPowerBackend,
) -> bool {
    if !calibration_runtime_state.immediate_heater_disarm_pending {
        return false;
    }
    backend.set_terminal_fixed_pd_disarmed(true);
    true
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn release_terminal_fixed_pd_disarm_for_manual_pps(
    backend: &mut HeaterPowerBackend,
    manual_pps_active: bool,
) -> bool {
    if !manual_pps_active {
        return false;
    }

    match backend {
        HeaterPowerBackend::PpsMos {
            terminal_fixed_pd_disarmed,
            current_mode,
            current_request_mv,
            settle_until_ms,
            next_request_at_ms,
            current_limit_fixed_pwm_active,
            current_limit_fixed_request_confirmed,
            idle_request_mv,
            ..
        } if *terminal_fixed_pd_disarmed => {
            // A new manual PPS request is an explicit, non-heating re-arm. It
            // may renegotiate the source while the heater output remains at zero.
            *terminal_fixed_pd_disarmed = false;
            *current_mode = None;
            *current_request_mv = *idle_request_mv;
            *settle_until_ms = None;
            *next_request_at_ms = 0;
            *current_limit_fixed_pwm_active = false;
            *current_limit_fixed_request_confirmed = false;
            true
        }
        HeaterPowerBackend::FixedPdPwmFallback {
            terminal_fixed_pd_disarmed,
            fixed_request_confirmed,
            ..
        } if *terminal_fixed_pd_disarmed => {
            // Leave fallback ready to request fixed PD again if the manual PPS
            // override is later cleared.
            *terminal_fixed_pd_disarmed = false;
            *fixed_request_confirmed = false;
            true
        }
        _ => false,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn manual_pps_request_required(
    manual_pps: ManualPpsState,
    controller: ControllerKind,
    observation: Option<PdStatusObservation>,
) -> bool {
    let Some(target_mv) = manual_pps.target_mv else {
        return false;
    };
    if manual_pps.applied_mv != Some(target_mv) {
        return true;
    }
    if controller != ControllerKind::Fusb302b {
        return false;
    }
    let target_ma = manual_pps.target_ma.unwrap_or(0);
    !observation.is_some_and(|observation| {
        observation.contract.kind == ContractKind::Pps
            && observation.contract.voltage_mv == target_mv
            && observation.contract.current_ma >= target_ma
    })
}
