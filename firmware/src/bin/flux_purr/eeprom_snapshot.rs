#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
#[derive(Debug, Default)]
struct EepromSnapshotSession {
    active: bool,
    session_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    next_offset: u16,
    last_activity_ms: u64,
}
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn snapshot_string<const N: usize>(value: &str) -> heapless::String<N> {
    let mut output = heapless::String::new();
    let _ = output.push_str(value);
    output
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn eeprom_snapshot_error(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    code: &'static str,
) -> EepromSnapshotResponse {
    EepromSnapshotResponse {
        ok: false,
        request_id,
        session_id: None,
        capacity: None,
        chunk_max: None,
        offset: None,
        bytes: None,
        sha256: None,
        error: Some(snapshot_string(code)),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn eeprom_snapshot_storage_failure(response: &EepromSnapshotResponse) -> bool {
    matches!(
        response.error.as_ref().map(|error| error.as_str()),
        Some("eeprom_unavailable" | "eeprom_read_failed" | "snapshot_hash_mismatch")
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
async fn eeprom_snapshot_digest<PWM>(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    service: &mut EepromPdServiceContext<'_, PWM>,
) -> Result<heapless::String<EEPROM_SNAPSHOT_HASH_LEN>, &'static str>
where
    PWM: SetDutyCycle,
{
    let Some(address) = probe_eeprom_address(i2c) else {
        return Err("eeprom_unavailable");
    };
    let mut hasher = Sha256::new();
    let mut offset = 0_u16;
    let mut bytes = [0_u8; EEPROM_SNAPSHOT_CHUNK_MAX as usize];
    while offset < EEPROM_SNAPSHOT_SIZE {
        let length = usize::from((EEPROM_SNAPSHOT_SIZE - offset).min(EEPROM_SNAPSHOT_CHUNK_MAX));
        read_eeprom_bytes_chunked_with_pd(
            i2c,
            pd_port,
            service,
            address,
            offset,
            &mut bytes[..length],
        )
        .await
        .map_err(|_| "eeprom_read_failed")?;
        hasher.update(&bytes[..length]);
        offset = offset.saturating_add(length as u16);
    }
    let digest = hasher.finalize();
    let mut rendered = heapless::String::new();
    rendered
        .push_str("sha256:")
        .map_err(|_| "digest_format_failed")?;
    for byte in digest {
        write!(rendered, "{byte:02x}").map_err(|_| "digest_format_failed")?;
    }
    Ok(rendered)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn write_eeprom_snapshot_response(
    usb: &mut RawUsbSerialJtag,
    response: &EepromSnapshotResponse,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
) {
    let Ok(written) = serde_json_core::to_slice(response, tx_buf) else {
        let _ = usb_write_bytes_bounded(usb, b"{\"ok\":false,\"error\":\"output_too_small\"}\n");
        return;
    };
    if written >= tx_buf.len() {
        let _ = usb_write_bytes_bounded(usb, b"{\"ok\":false,\"error\":\"output_too_small\"}\n");
        return;
    }
    tx_buf[written] = b'\n';
    let _ = usb_write_bytes_bounded(usb, &tx_buf[..=written]);
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
#[expect(
    clippy::too_many_lines,
    reason = "snapshot protocol handling keeps framing and persistence ordering together"
)]
async fn process_eeprom_snapshot_line<PWM>(
    line: &str,
    session: &mut EepromSnapshotSession,
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    service: &mut EepromPdServiceContext<'_, PWM>,
    memory_commit_due_ms: &mut Option<u64>,
    elapsed_ms: u64,
) -> Option<EepromSnapshotResponse>
where
    PWM: SetDutyCycle,
{
    let parsed = serde_json_core::from_slice::<EepromSnapshotRequest>(line.as_bytes());
    let request = match parsed {
        Ok((request, _)) if request.op.as_str().starts_with("eeprom_snapshot_") => request,
        Ok(_) => return None,
        Err(_) if line.contains("eeprom_snapshot_") => {
            return Some(eeprom_snapshot_error(
                heapless::String::new(),
                "malformed_snapshot",
            ));
        }
        Err(_) => return None,
    };
    if session.active
        && elapsed_ms.saturating_sub(session.last_activity_ms) > EEPROM_SNAPSHOT_TIMEOUT_MS
    {
        session.active = false;
        session.session_id.clear();
        session.next_offset = 0;
        *memory_commit_due_ms = None;
    }
    let request_id = request.request_id.clone();
    let requested_session = request
        .session_id
        .as_ref()
        .unwrap_or(&request.request_id)
        .clone();
    match request.op.as_str() {
        "eeprom_snapshot_open" => {
            if *service.last_heater_duty != 0 {
                return Some(eeprom_snapshot_error(request_id, "heater_active"));
            }
            let session_id = request.session_id.unwrap_or(request.request_id.clone());
            if session_id.is_empty() {
                return Some(eeprom_snapshot_error(request_id, "session_required"));
            }
            session.active = true;
            session.session_id = session_id.clone();
            session.next_offset = 0;
            session.last_activity_ms = elapsed_ms;
            *memory_commit_due_ms = None;
            Some(EepromSnapshotResponse {
                ok: true,
                request_id,
                session_id: Some(session_id),
                capacity: Some(EEPROM_SNAPSHOT_SIZE),
                chunk_max: Some(EEPROM_SNAPSHOT_CHUNK_MAX),
                offset: None,
                bytes: None,
                sha256: None,
                error: None,
            })
        }
        "eeprom_snapshot_read" => {
            if !session.active || requested_session != session.session_id {
                return Some(eeprom_snapshot_error(
                    request_id,
                    "snapshot_session_invalid",
                ));
            }
            if *service.last_heater_duty != 0 {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "heater_active"));
            }
            let (Some(offset), Some(length)) = (request.offset, request.length) else {
                return Some(eeprom_snapshot_error(request_id, "snapshot_range_required"));
            };
            if length == 0
                || length > EEPROM_SNAPSHOT_CHUNK_MAX
                || offset != session.next_offset
                || offset.saturating_add(length) > EEPROM_SNAPSHOT_SIZE
            {
                return Some(eeprom_snapshot_error(request_id, "snapshot_range_invalid"));
            }
            let Some(address) = probe_eeprom_address(i2c) else {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "eeprom_unavailable"));
            };
            let mut bytes = heapless::Vec::<u8, 32>::new();
            let _ = bytes.resize_default(usize::from(length));
            if read_eeprom_bytes_chunked_with_pd(
                i2c,
                pd_port,
                service,
                address,
                offset,
                bytes.as_mut_slice(),
            )
            .await
            .is_err()
            {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "eeprom_read_failed"));
            }
            session.next_offset = session.next_offset.saturating_add(length);
            session.last_activity_ms = elapsed_ms;
            *memory_commit_due_ms = None;
            Some(EepromSnapshotResponse {
                ok: true,
                request_id,
                session_id: Some(session.session_id.clone()),
                capacity: None,
                chunk_max: None,
                offset: Some(offset),
                bytes: Some(bytes),
                sha256: None,
                error: None,
            })
        }
        "eeprom_snapshot_close" => {
            if !session.active || requested_session != session.session_id {
                return Some(eeprom_snapshot_error(
                    request_id,
                    "snapshot_session_invalid",
                ));
            }
            if *service.last_heater_duty != 0 {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "heater_active"));
            }
            if session.next_offset != EEPROM_SNAPSHOT_SIZE {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "snapshot_incomplete"));
            }
            let digest = match eeprom_snapshot_digest(i2c, pd_port, service).await {
                Ok(digest) => digest,
                Err(code) => {
                    session.active = false;
                    return Some(eeprom_snapshot_error(request_id, code));
                }
            };
            if request.sha256.as_ref() != Some(&digest) {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "snapshot_hash_mismatch"));
            }
            let session_id = session.session_id.clone();
            session.active = false;
            session.session_id.clear();
            session.next_offset = 0;
            session.last_activity_ms = elapsed_ms;
            *memory_commit_due_ms = None;
            Some(EepromSnapshotResponse {
                ok: true,
                request_id,
                session_id: Some(session_id),
                capacity: None,
                chunk_max: None,
                offset: None,
                bytes: None,
                sha256: Some(digest),
                error: None,
            })
        }
        _ => Some(eeprom_snapshot_error(request_id, "snapshot_op_unsupported")),
    }
}
