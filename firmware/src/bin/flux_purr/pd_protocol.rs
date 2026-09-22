#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
const fn fusb302b_i2c_error_kind(error: fusb302::Error<PdI2cError>) -> u8 {
    use esp_hal::i2c::master::{AcknowledgeCheckFailedReason, Error as HalI2cError};

    match error {
        fusb302::Error::I2c(PdI2cError::BusBusy) => FUSB302B_I2C_ERROR_BUS_BUSY,
        fusb302::Error::I2c(PdI2cError::I2c(HalI2cError::AcknowledgeCheckFailed(
            AcknowledgeCheckFailedReason::Address,
        ))) => FUSB302B_I2C_ERROR_ACK_ADDRESS,
        fusb302::Error::I2c(PdI2cError::I2c(HalI2cError::AcknowledgeCheckFailed(
            AcknowledgeCheckFailedReason::Data,
        ))) => FUSB302B_I2C_ERROR_ACK_DATA,
        fusb302::Error::I2c(PdI2cError::I2c(HalI2cError::AcknowledgeCheckFailed(
            AcknowledgeCheckFailedReason::Unknown,
        ))) => FUSB302B_I2C_ERROR_ACK_UNKNOWN,
        fusb302::Error::I2c(PdI2cError::I2c(HalI2cError::Timeout)) => FUSB302B_I2C_ERROR_TIMEOUT,
        fusb302::Error::I2c(PdI2cError::I2c(HalI2cError::ArbitrationLost)) => {
            FUSB302B_I2C_ERROR_ARBITRATION_LOST
        }
        fusb302::Error::I2c(PdI2cError::I2c(HalI2cError::ExecutionIncomplete)) => {
            FUSB302B_I2C_ERROR_EXECUTION_INCOMPLETE
        }
        _ => FUSB302B_I2C_ERROR_OTHER,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn fusb302b_receive_event(
    i2c: &mut PdI2c<'_>,
    retry_fail_recovery_pending: bool,
) -> Result<Fusb302bReceiveEvent, Fusb302bReceiveFault> {
    let mut phy = Fusb302::new(&mut *i2c);
    // Clear transition latches first, then sample the non-destructive status
    // bank. This pairs a VBUSOK detach transition with its current low level
    // instead of leaving a status/interrupt race between the two I2C reads.
    let interrupts = phy
        .read_interrupts()
        .await
        .map_err(|error| Fusb302bReceiveFault::InterruptRead(fusb302b_i2c_error_kind(error)))?;
    let status = phy
        .read_status()
        .await
        .map_err(|error| Fusb302bReceiveFault::StatusRead(fusb302b_i2c_error_kind(error)))?;
    let tx_sent = interrupts.interrupt_a & FUSB302B_INTERRUPTA_TX_SENT != 0;
    let gcrc_sent = interrupts.interrupt_b & FUSB302B_INTERRUPTB_GCRC_SENT != 0;

    if status.status0 & FUSB302B_STATUS0_VBUSOK == 0 {
        return Ok(Fusb302bReceiveEvent::VbusLow {
            transition: fusb302b_vbus_detach_was_reported(interrupts.interrupt, status.status0),
        });
    }
    if let Some(action) = fusb302b_received_reset_action(interrupts.interrupt_a) {
        return Ok(Fusb302bReceiveEvent::ReceivedReset(action));
    }
    if fusb302b_retry_failure_requires_recovery(
        status.status0a,
        status.status1,
        retry_fail_recovery_pending,
    ) {
        return Ok(Fusb302bReceiveEvent::RetryFailed);
    }
    if status.status1 & (FUSB302B_STATUS1_OVERTEMP | FUSB302B_STATUS1_VCONN_OCP) != 0 {
        return Ok(Fusb302bReceiveEvent::Protection);
    }
    if fusb302b_retry_recovery_should_discard_frame(status.status1, retry_fail_recovery_pending) {
        if !fusb302b_flush_receive_fifo(i2c).await {
            return Err(Fusb302bReceiveFault::ReceiveFifoFlush);
        }
        return Ok(Fusb302bReceiveEvent::Empty { tx_sent, gcrc_sent });
    }
    if status.status1 & FUSB302B_STATUS1_RX_EMPTY != 0 {
        return Ok(Fusb302bReceiveEvent::Empty { tx_sent, gcrc_sent });
    }
    if status.status0 & FUSB302B_STATUS0_CRC_CHECK == 0
        || status.status1a & FUSB302B_STATUS1A_RXSOP == 0
    {
        return Ok(Fusb302bReceiveEvent::Partial { tx_sent, gcrc_sent });
    }

    match phy.receive().await {
        Ok(None) => Ok(Fusb302bReceiveEvent::Empty { tx_sent, gcrc_sent }),
        Ok(Some(packet)) if packet.sop() == SopType::Sop => {
            Ok(Fusb302bReceiveEvent::Message(packet))
        }
        Ok(Some(_)) => Ok(Fusb302bReceiveEvent::UnsupportedSop),
        Err(fusb302::Error::Receive(_)) => Ok(Fusb302bReceiveEvent::UnsupportedSop),
        Err(error) => Err(Fusb302bReceiveFault::PacketReceive(
            fusb302b_i2c_error_kind(error),
        )),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fusb302b_adjustable_power_capabilities(
    source_capabilities: SourceCapabilities,
) -> Option<ch224q::AdjustablePowerCapabilities> {
    let mut capabilities = ch224q::AdjustablePowerCapabilities::default();
    let mut has_usable_pps_apdo = false;

    for apdo in source_capabilities.pps.into_iter().flatten() {
        let min_mv = apdo.min_mv.max(FUSB302B_PPS_MIN_MV);
        let max_mv = apdo.max_mv.min(FUSB302B_PPS_MAX_MV);
        let max_ma = apdo.max_ma.min(MAX_HEATER_CONTRACT_MA);
        if min_mv > max_mv || max_ma < MIN_HEATER_CONTRACT_MA {
            continue;
        }
        has_usable_pps_apdo = true;

        capabilities.pps_min_mv = Some(
            capabilities
                .pps_min_mv
                .map_or(min_mv, |value| value.min(min_mv)),
        );
        capabilities.pps_max_mv = Some(
            capabilities
                .pps_max_mv
                .map_or(max_mv, |value| value.max(max_mv)),
        );
        capabilities.pps_max_ma = Some(
            capabilities
                .pps_max_ma
                .map_or(max_ma, |value| value.max(max_ma)),
        );
        capabilities.pps_covers_20v |=
            min_mv <= GUARANTEED_HEATER_MIN_MV && max_mv >= GUARANTEED_HEATER_MIN_MV;
        if let Some(slot) = capabilities
            .pps_apdos
            .iter_mut()
            .find(|slot| slot.is_none())
        {
            *slot = Some(ch224q::PpsApdo {
                min_mv,
                max_mv,
                max_ma,
            });
        }
    }

    has_usable_pps_apdo.then_some(capabilities)
}

#[cfg(target_arch = "xtensa")]
pub(crate) type PdPort = PdServiceClient;

#[cfg(target_arch = "xtensa")]
pub(crate) enum DetectedPdController {
    Fusb302b(u8),
    Unknown,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fusb302b_identity_is_stable(
    first_id: Option<u8>,
    second_id: Option<u8>,
    status0: Option<u8>,
    status1: Option<u8>,
) -> bool {
    matches!((first_id, second_id, status0, status1), (Some(first), Some(second), Some(status0), Some(status1))
        if first == second
            && first & 0xf0 == 0x90
            && status0 != u8::MAX
            && status1 != u8::MAX)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn detect_pd_controller(i2c: &mut PdI2c<'_>) -> DetectedPdController {
    let first = {
        let mut phy = Fusb302::new(&mut *i2c);
        phy.device_id().await.ok()
    };
    let second = {
        let mut phy = Fusb302::new(&mut *i2c);
        phy.device_id().await.ok()
    };
    let (Some(first), Some(second)) = (first, second) else {
        return DetectedPdController::Unknown;
    };
    if first != second || !first.is_fusb302b_family() {
        return DetectedPdController::Unknown;
    }
    let status = {
        let mut phy = Fusb302::new(&mut *i2c);
        phy.read_status().await.ok()
    };
    let (status0, status1) = status
        .map(|status| (Some(status.status0), Some(status.status1)))
        .unwrap_or((None, None));
    if fusb302b_identity_is_stable(Some(first.bits()), Some(second.bits()), status0, status1) {
        DetectedPdController::Fusb302b(first.bits())
    } else {
        DetectedPdController::Unknown
    }
}
