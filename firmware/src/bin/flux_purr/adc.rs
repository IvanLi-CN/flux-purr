#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
use embedded_hal_async::i2c::I2c as _;

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn vin_adc_mv_for_input_mv(input_mv: u32) -> u16 {
    let numerator = input_mv.saturating_mul(s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS);
    let denominator =
        s3_frontpanel::VIN_DIVIDER_R_HIGH_OHMS + s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS;
    (numerator / denominator).min(u32::from(u16::MAX)) as u16
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn vin_input_mv_from_adc_mv(adc_mv: u16) -> u32 {
    let numerator = u32::from(adc_mv).saturating_mul(
        s3_frontpanel::VIN_DIVIDER_R_HIGH_OHMS + s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS,
    );
    numerator / s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn rtd_fractional_mean_mv(sum_mv: u32, valid_samples: usize) -> Option<f32> {
    if valid_samples < RTD_MIN_VALID_SAMPLE_COUNT {
        return None;
    }
    Some(sum_mv as f32 / valid_samples as f32)
}

#[cfg(test)]
pub(crate) fn oversampled_fractional_mean_mv_with_discard<F>(
    total_samples: usize,
    discard_valid_prefix_samples: usize,
    read_sample: F,
) -> Option<f32>
where
    F: FnMut() -> Option<u16>,
{
    oversampled_rtd_batch_with_discard(total_samples, discard_valid_prefix_samples, read_sample)
        .map(|batch| batch.mean_mv)
}

#[cfg(test)]
pub(crate) fn oversampled_rtd_batch_with_discard<F>(
    total_samples: usize,
    discard_valid_prefix_samples: usize,
    mut read_sample: F,
) -> Option<RtdAdcBatch>
where
    F: FnMut() -> Option<u16>,
{
    let mut sum_mv: u32 = 0;
    let mut min_mv = u16::MAX;
    let mut max_mv = 0_u16;
    let mut valid_samples = 0_usize;
    let mut discarded_valid_samples = 0_usize;

    for _ in 0..total_samples {
        let Some(sample_mv) = read_sample() else {
            continue;
        };
        if discarded_valid_samples < discard_valid_prefix_samples {
            discarded_valid_samples = discarded_valid_samples.saturating_add(1);
            continue;
        }
        sum_mv = sum_mv.saturating_add(sample_mv as u32);
        min_mv = min_mv.min(sample_mv);
        max_mv = max_mv.max(sample_mv);
        valid_samples = valid_samples.saturating_add(1);
    }

    rtd_fractional_mean_mv(sum_mv, valid_samples).map(|mean_mv| RtdAdcBatch {
        mean_mv,
        min_mv,
        max_mv,
        mean_raw_code: mean_mv.round() as u16,
        min_raw_code: min_mv,
        max_raw_code: max_mv,
    })
}

#[cfg(test)]
pub(crate) fn phase_averaged_rtd_batch_with_discard<F, W>(
    retained_samples: usize,
    phase_count: usize,
    discard_valid_prefix_samples: usize,
    mut read_sample: F,
    mut wait_for_next_phase: W,
) -> Option<RtdAdcBatch>
where
    F: FnMut() -> Option<AdcConvertedSample>,
    W: FnMut(),
{
    if retained_samples == 0 || phase_count == 0 || !retained_samples.is_multiple_of(phase_count) {
        return None;
    }

    let samples_per_phase = retained_samples / phase_count;
    let mut sum_mv: u32 = 0;
    let mut sum_raw_code: u32 = 0;
    let mut min_mv = u16::MAX;
    let mut max_mv = 0_u16;
    let mut min_raw_code = u16::MAX;
    let mut max_raw_code = 0_u16;
    let mut valid_samples = 0_usize;
    let mut discarded_valid_samples = 0_usize;

    for _ in 0..retained_samples.saturating_add(discard_valid_prefix_samples) {
        let Some(sample) = read_sample() else {
            continue;
        };
        if discarded_valid_samples < discard_valid_prefix_samples {
            discarded_valid_samples = discarded_valid_samples.saturating_add(1);
            continue;
        }

        sum_mv = sum_mv.saturating_add(sample.calibrated_mv as u32);
        sum_raw_code = sum_raw_code.saturating_add(sample.raw_code as u32);
        min_mv = min_mv.min(sample.calibrated_mv);
        max_mv = max_mv.max(sample.calibrated_mv);
        min_raw_code = min_raw_code.min(sample.raw_code);
        max_raw_code = max_raw_code.max(sample.raw_code);
        valid_samples = valid_samples.saturating_add(1);

        if valid_samples.is_multiple_of(samples_per_phase) && valid_samples < retained_samples {
            wait_for_next_phase();
        }
    }

    rtd_fractional_mean_mv(sum_mv, valid_samples).map(|mean_mv| RtdAdcBatch {
        mean_mv,
        min_mv,
        max_mv,
        mean_raw_code: (sum_raw_code / valid_samples as u32) as u16,
        min_raw_code,
        max_raw_code,
    })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn mask_adc1_raw_code(value: u16) -> u16 {
    value & 0x0fff
}

#[cfg(target_arch = "xtensa")]
pub(crate) type Adc1Driver = Adc<'static, esp_hal::peripherals::ADC1<'static>, esp_hal::Blocking>;
#[cfg(target_arch = "xtensa")]
pub(crate) type VinAdcPin = esp_hal::analog::adc::AdcPin<
    esp_hal::peripherals::GPIO1<'static>,
    esp_hal::peripherals::ADC1<'static>,
    AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
>;
#[cfg(target_arch = "xtensa")]
pub(crate) type RtdAdcPin = esp_hal::analog::adc::AdcPin<
    esp_hal::peripherals::GPIO2<'static>,
    esp_hal::peripherals::ADC1<'static>,
    AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
>;
#[cfg(target_arch = "xtensa")]
pub(crate) type Adc1Curve = AdcCalCurve<esp_hal::peripherals::ADC1<'static>>;

#[cfg(target_arch = "xtensa")]
pub(crate) fn initialize_adc1(
    adc: esp_hal::peripherals::ADC1<'static>,
    vin_gpio: esp_hal::peripherals::GPIO1<'static>,
    rtd_gpio: esp_hal::peripherals::GPIO2<'static>,
) -> (Adc1Driver, VinAdcPin, RtdAdcPin, Option<Adc1Curve>) {
    let efuse_version = Efuse::rtc_calib_version();
    let init_code = Efuse::rtc_calib_init_code(AdcCalibUnit::ADC1, RTD_SAMPLE_ATTENUATION);
    let reference_code = Efuse::rtc_calib_cal_code(AdcCalibUnit::ADC1, RTD_SAMPLE_ATTENUATION);
    let reference_mv = (efuse_version == 1)
        .then(|| Efuse::rtc_calib_cal_mv(AdcCalibUnit::ADC1, RTD_SAMPLE_ATTENUATION));
    let efuse_ready = efuse_version == 1
        && init_code.is_some()
        && reference_code.is_some()
        && reference_mv.is_some();
    #[cfg(feature = "web_serial")]
    {
        ADC_CALIBRATION_SOURCE.store(if efuse_ready { 0 } else { 1 }, Ordering::Relaxed);
        ADC_EFUSE_VERSION.store(efuse_version, Ordering::Relaxed);
        ADC_INIT_CODE.store(init_code.unwrap_or(u16::MAX), Ordering::Relaxed);
        ADC_REFERENCE_CODE.store(reference_code.unwrap_or(u16::MAX), Ordering::Relaxed);
        ADC_REFERENCE_MV.store(reference_mv.unwrap_or(u16::MAX), Ordering::Relaxed);
    }

    let mut config = AdcConfig::new();
    let vin_pin = config.enable_pin_with_cal::<_, AdcCalBasic<_>>(vin_gpio, RTD_SAMPLE_ATTENUATION);
    let rtd_pin = config.enable_pin_with_cal::<_, AdcCalBasic<_>>(rtd_gpio, RTD_SAMPLE_ATTENUATION);
    let adc = Adc::new(adc, config);
    let curve = efuse_ready.then(|| Adc1Curve::new_cal(RTD_SAMPLE_ATTENUATION));
    (adc, vin_pin, rtd_pin, curve)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn read_adc_sample<PIN>(
    adc: &mut Adc1Driver,
    pin: &mut esp_hal::analog::adc::AdcPin<
        PIN,
        esp_hal::peripherals::ADC1<'static>,
        AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
    >,
    curve: &Adc1Curve,
) -> Option<AdcConvertedSample>
where
    PIN: AdcChannel,
{
    loop {
        match adc.read_oneshot(pin) {
            Ok(value) => {
                let raw_code = mask_adc1_raw_code(value);
                return Some(AdcConvertedSample {
                    raw_code,
                    calibrated_mv: curve.adc_val(raw_code),
                });
            }
            Err(nb::Error::WouldBlock) => EmbassyTimer::after_micros(50).await,
            Err(_) => return None,
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn wait_for_adc_phase_if_needed(should_wait: bool) {
    if should_wait {
        EmbassyTimer::after_micros(u64::from(RTD_SAMPLE_PWM_PHASE_SPACING_US)).await;
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn read_adc_batch<PIN>(
    adc: &mut Adc1Driver,
    pin: &mut esp_hal::analog::adc::AdcPin<
        PIN,
        esp_hal::peripherals::ADC1<'static>,
        AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
    >,
    curve: &Adc1Curve,
) -> Option<RtdAdcBatch>
where
    PIN: AdcChannel,
{
    let samples_per_phase = RTD_SAMPLE_COUNT / RTD_SAMPLE_PWM_PHASE_COUNT;
    let total_samples = RTD_SAMPLE_COUNT.saturating_add(RTD_SETTLE_DISCARD_SAMPLE_COUNT);
    let mut sum_mv = 0_u32;
    let mut sum_raw_code = 0_u32;
    let mut min_mv = u16::MAX;
    let mut max_mv = 0_u16;
    let mut min_raw_code = u16::MAX;
    let mut max_raw_code = 0_u16;
    let mut valid_samples = 0_usize;
    let mut discarded_valid_samples = 0_usize;
    // Keep the calibrated acquisition plan intact while yielding between
    // conversions. The dedicated PD task runs independently while this batch
    // is active.
    let settle_deadline = Instant::now().checked_add(Duration::from_micros(u64::from(
        RTD_CHANNEL_SWITCH_SETTLE_US,
    )))?;
    while Instant::now() < settle_deadline {
        EmbassyTimer::after_millis(1).await;
    }

    for _ in 0..total_samples {
        let sample = read_adc_sample(adc, pin, curve).await;

        if let Some(sample) = sample {
            if discarded_valid_samples < RTD_SETTLE_DISCARD_SAMPLE_COUNT {
                discarded_valid_samples = discarded_valid_samples.saturating_add(1);
            } else {
                sum_mv = sum_mv.saturating_add(u32::from(sample.calibrated_mv));
                sum_raw_code = sum_raw_code.saturating_add(u32::from(sample.raw_code));
                min_mv = min_mv.min(sample.calibrated_mv);
                max_mv = max_mv.max(sample.calibrated_mv);
                min_raw_code = min_raw_code.min(sample.raw_code);
                max_raw_code = max_raw_code.max(sample.raw_code);
                valid_samples = valid_samples.saturating_add(1);
                wait_for_adc_phase_if_needed(
                    valid_samples.is_multiple_of(samples_per_phase)
                        && valid_samples < RTD_SAMPLE_COUNT,
                )
                .await;
            }
        }
    }

    rtd_fractional_mean_mv(sum_mv, valid_samples).map(|mean_mv| RtdAdcBatch {
        mean_mv,
        min_mv,
        max_mv,
        mean_raw_code: (sum_raw_code / valid_samples as u32) as u16,
        min_raw_code,
        max_raw_code,
    })
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn read_calibrated_vin_mv(
    adc: &mut Adc1Driver,
    pin: &mut VinAdcPin,
    curve: Option<&Adc1Curve>,
    memory_config: &MemoryConfig,
) -> Option<(u16, u16, u16, u32)> {
    let curve = curve?;
    let batch = read_adc_batch(adc, pin, curve).await?;
    let raw_code = batch.mean_raw_code;
    let raw_adc_mv = batch.mean_mv.round() as u16;
    #[cfg(feature = "web_serial")]
    VIN_RAW_CODE_MEAN.store(raw_code, Ordering::Relaxed);
    let corrected_adc_mv = correct_adc_mv(
        &memory_config.adc_calibration,
        AdcCalibrationChannel::Vin,
        raw_adc_mv,
    );
    Some((
        raw_code,
        raw_adc_mv,
        corrected_adc_mv,
        vin_input_mv_from_adc_mv(corrected_adc_mv),
    ))
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn read_rtd_sample(
    adc: &mut Adc1Driver,
    pin: &mut RtdAdcPin,
    curve: Option<&Adc1Curve>,
    memory_config: &MemoryConfig,
) -> RtdSample {
    let Some(curve) = curve else {
        return RtdSample::Fault {
            adc_mv: None,
            reason: HeaterFaultReason::AdcReadFailed,
        };
    };
    let Some(batch) = read_adc_batch(adc, pin, curve).await else {
        return RtdSample::Fault {
            adc_mv: None,
            reason: HeaterFaultReason::AdcReadFailed,
        };
    };
    #[cfg(feature = "web_serial")]
    {
        RTD_RAW_CODE_MEAN.store(batch.mean_raw_code, Ordering::Relaxed);
        RTD_RAW_CODE_MIN.store(batch.min_raw_code, Ordering::Relaxed);
        RTD_RAW_CODE_MAX.store(batch.max_raw_code, Ordering::Relaxed);
    }
    let raw_adc_mv = batch.mean_mv.round() as u16;
    let raw_adc_fractional_mv = batch.mean_mv;

    if raw_adc_fractional_mv <= f32::from(RTD_SHORT_FAULT_MAX_MV) {
        return RtdSample::Fault {
            adc_mv: Some(raw_adc_mv),
            reason: HeaterFaultReason::SensorShort,
        };
    }
    if raw_adc_fractional_mv >= f32::from(RTD_OPEN_FAULT_MIN_MV) {
        return RtdSample::Fault {
            adc_mv: Some(raw_adc_mv),
            reason: HeaterFaultReason::SensorOpen,
        };
    }

    let adc_fractional_mv = correct_adc_fractional_mv(
        memory_config,
        AdcCalibrationChannel::Rtd,
        raw_adc_fractional_mv,
    );
    let adc_mv = adc_fractional_mv.round() as u16;

    match rtd_resistance_ohms_from_fractional_mv(adc_fractional_mv) {
        Ok(resistance_ohms) => {
            let temp_c = pt1000_temperature_c_from_resistance(resistance_ohms);
            RtdSample::Valid(RtdMeasurement {
                raw_adc_mv,
                raw_adc_min_mv: batch.min_mv,
                raw_adc_max_mv: batch.max_mv,
                adc_mv,
                resistance_ohms,
                temp_c,
                current_temp_c: temp_c_to_whole_c(temp_c),
            })
        }
        Err(reason) => RtdSample::Fault {
            adc_mv: Some(adc_mv),
            reason,
        },
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_STATUS0_CRC_CHECK: u8 = 1 << 4;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_STATUS0A_RETRY_FAIL: u8 = 1 << 4;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_STATUS0_VBUSOK: u8 = 1 << 7;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_STATUS1_RX_EMPTY: u8 = 1 << 5;
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_STATUS1_OVERTEMP: u8 = 1 << 1;
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_STATUS1_VCONN_OCP: u8 = 1;
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_STATUS1A_RXSOP: u8 = 1;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_TOGSS_MASK: u8 = 0b0011_1000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_TOGSS_SNK_CC1: u8 = 0b0010_1000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_TOGSS_SNK_CC2: u8 = 0b0011_0000;
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_INTERRUPTA_TX_SENT: u8 = 1 << 2;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_INTERRUPT_VBUSOK: u8 = 1 << 7;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_VBUS_LOW_CONFIRM_MS: u64 = 50;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_VBUS_RESTORE_CONFIRM_MS: u64 = 50;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_INTERRUPTA_SOFT_RESET: u8 = 1 << 1;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_INTERRUPTA_HARD_RESET: u8 = 1;
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_INTERRUPTB_GCRC_SENT: u8 = 1;
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_CONTROL1_REGISTER: u8 = 0x07;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_CONTROL1_RW_MASK: u8 = 0b0111_0011;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_CONTROL1_RX_FLUSH: u8 = 1 << 2;
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_CONTROL0_REGISTER: u8 = 0x06;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_CONTROL0_RW_MASK: u8 = 0b0010_1110;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_CONTROL0_TX_FLUSH: u8 = 1 << 6;
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_TOGGLE_INTERRUPT_MASKS: InterruptMasks =
    InterruptMasks::new(0x7f, 0xbf, 0xff);
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_RECEIVE_INTERRUPT_MASKS: InterruptMasks =
    InterruptMasks::new(0x7d, 0xe0, 0x00);

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn fusb302b_settled_sink_polarity(status1a: u8) -> Option<u8> {
    match status1a & FUSB302B_TOGSS_MASK {
        FUSB302B_TOGSS_SNK_CC1 => Some(1),
        FUSB302B_TOGSS_SNK_CC2 => Some(2),
        _ => None,
    }
}

/// A powered FUSB302B starts a possible Sink detach with the VBUSOK transition
/// interrupt. The low level is then sampled on later service turns; a single
/// transient must not restart the CC session.
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn fusb302b_vbus_detach_was_reported(interrupt: u8, status0: u8) -> bool {
    interrupt & FUSB302B_INTERRUPT_VBUSOK != 0 && status0 & FUSB302B_STATUS0_VBUSOK == 0
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn fusb302b_vbus_low_confirmation_expired(
    candidate_since_ms: Option<u64>,
    now_ms: u64,
) -> bool {
    match candidate_since_ms {
        Some(started) => now_ms.saturating_sub(started) >= FUSB302B_VBUS_LOW_CONFIRM_MS,
        None => false,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn fusb302b_vbus_restore_confirmation_expired(
    candidate_since_ms: Option<u64>,
    now_ms: u64,
) -> bool {
    match candidate_since_ms {
        Some(started) => now_ms.saturating_sub(started) >= FUSB302B_VBUS_RESTORE_CONFIRM_MS,
        None => false,
    }
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fusb302bReceiveEvent {
    Empty { tx_sent: bool, gcrc_sent: bool },
    Partial { tx_sent: bool, gcrc_sent: bool },
    Message(PdPacket),
    VbusLow { transition: bool },
    ReceivedReset(Fusb302bReceivedResetAction),
    RetryFailed,
    Protection,
    UnsupportedSop,
}

/// The FUSB302B reports received PD resets independently of Type-C CC state.
/// Neither event is evidence that the attached source has detached.
#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fusb302bReceivedResetAction {
    AcceptAndWaitForSourceCapabilities,
    WaitForSourceCapabilities,
}

#[cfg(target_arch = "xtensa")]
pub(crate) const fn fusb302b_phy_config(auto_goodcrc: bool) -> PhyConfig {
    PhyConfig {
        pd_revision: PdRevision::Rev30,
        power_role: PowerRole::Sink,
        data_role: DataRole::Ufp,
        auto_goodcrc,
        retry_count: RetryCount::Three,
        auto_soft_reset: false,
        auto_hard_reset: false,
        receive_sop: fusb302::ReceiveSopMask::NONE,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn fusb302b_received_reset_action(
    interrupt_a: u8,
) -> Option<Fusb302bReceivedResetAction> {
    if interrupt_a & FUSB302B_INTERRUPTA_HARD_RESET != 0 {
        Some(Fusb302bReceivedResetAction::WaitForSourceCapabilities)
    } else if interrupt_a & FUSB302B_INTERRUPTA_SOFT_RESET != 0 {
        Some(Fusb302bReceivedResetAction::AcceptAndWaitForSourceCapabilities)
    } else {
        None
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn fusb302b_receive_fifo_flush_value(control1: u8) -> u8 {
    (control1 & FUSB302B_CONTROL1_RW_MASK) | FUSB302B_CONTROL1_RX_FLUSH
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn fusb302b_transmit_fifo_flush_value(control0: u8) -> u8 {
    (control0 & FUSB302B_CONTROL0_RW_MASK) | FUSB302B_CONTROL0_TX_FLUSH
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn fusb302b_retry_failure_requires_recovery(
    status0a: u8,
    _status1: u8,
    retry_fail_recovery_pending: bool,
) -> bool {
    !retry_fail_recovery_pending && status0a & FUSB302B_STATUS0A_RETRY_FAIL != 0
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn fusb302b_retry_recovery_should_discard_frame(
    status1: u8,
    retry_fail_recovery_pending: bool,
) -> bool {
    retry_fail_recovery_pending && status1 & FUSB302B_STATUS1_RX_EMPTY == 0
}

/// The upstream PHY API exposes only a combined FIFO flush. Receive recovery
/// therefore updates only CONTROL1.RX_FLUSH and preserves the driver's
/// receive-mask bits; transmit recovery uses the separate TX flush below.
#[cfg(target_arch = "xtensa")]
pub(crate) async fn fusb302b_flush_receive_fifo(i2c: &mut PdI2c<'_>) -> bool {
    let mut control1 = [0_u8];
    i2c.write_read(
        fusb302::DEFAULT_ADDRESS,
        &[FUSB302B_CONTROL1_REGISTER],
        &mut control1,
    )
    .await
    .is_ok()
        && i2c
            .write(
                fusb302::DEFAULT_ADDRESS,
                &[
                    FUSB302B_CONTROL1_REGISTER,
                    fusb302b_receive_fifo_flush_value(control1[0]),
                ],
            )
            .await
            .is_ok()
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn fusb302b_flush_transmit_fifo(i2c: &mut PdI2c<'_>) -> bool {
    let mut control0 = [0_u8];
    i2c.write_read(
        fusb302::DEFAULT_ADDRESS,
        &[FUSB302B_CONTROL0_REGISTER],
        &mut control0,
    )
    .await
    .is_ok()
        && i2c
            .write(
                fusb302::DEFAULT_ADDRESS,
                &[
                    FUSB302B_CONTROL0_REGISTER,
                    fusb302b_transmit_fifo_flush_value(control0[0]),
                ],
            )
            .await
            .is_ok()
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct Fusb302bRuntime {
    pub(crate) policy: fusb302b::SinkPolicy,
    pub(crate) polarity: Option<CcPin>,
    pub(crate) next_message_id: u8,
    pub(crate) attached_at_ms: Option<u64>,
    pub(crate) last_source_capabilities_request_at_ms: Option<u64>,
    pub(crate) source_capabilities_refresh_pending: bool,
    pub(crate) source_capabilities_refresh_for_contract: bool,
    pub(crate) source_capabilities_refresh_requested_at_ms: Option<u64>,
    pub(crate) last_request_at_ms: Option<u64>,
    pub(crate) source_capabilities_tx_confirmed: bool,
    pub(crate) source_capabilities_gcrc_seen: bool,
    pub(crate) partial_rx_started_at_ms: Option<u64>,
    pub(crate) retry_fail_recovery_pending: bool,
    pub(crate) vbus_low_candidate_since_ms: Option<u64>,
    pub(crate) vbus_low_interlocked: bool,
    pub(crate) vbus_restore_candidate_since_ms: Option<u64>,
    pub(crate) awaiting_vbus_restore: bool,
    pub(crate) request_rejected: bool,
    pub(crate) request_timed_out: bool,
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PdContractRequestState {
    Confirmed,
    Pending,
    Failed,
}

#[cfg(target_arch = "xtensa")]
impl Fusb302bRuntime {
    pub(crate) const fn new() -> Self {
        Self {
            policy: fusb302b::SinkPolicy::new(
                FUSB302B_INITIAL_PPS_REQUEST_MV,
                MIN_HEATER_CONTRACT_MA,
            ),
            polarity: None,
            next_message_id: 0,
            attached_at_ms: None,
            last_source_capabilities_request_at_ms: None,
            source_capabilities_refresh_pending: false,
            source_capabilities_refresh_for_contract: false,
            source_capabilities_refresh_requested_at_ms: None,
            last_request_at_ms: None,
            source_capabilities_tx_confirmed: false,
            source_capabilities_gcrc_seen: false,
            partial_rx_started_at_ms: None,
            retry_fail_recovery_pending: false,
            vbus_low_candidate_since_ms: None,
            vbus_low_interlocked: false,
            vbus_restore_candidate_since_ms: None,
            awaiting_vbus_restore: false,
            request_rejected: false,
            request_timed_out: false,
        }
    }

    fn clear_vbus_low_interlock(&mut self) {
        self.vbus_low_candidate_since_ms = None;
        self.vbus_low_interlocked = false;
    }

    fn clear_contract_authorization(&mut self, now_ms: u64) {
        self.policy.on_received_protocol_reset();
        self.attached_at_ms = Some(now_ms);
        self.last_source_capabilities_request_at_ms = None;
        self.source_capabilities_refresh_pending = false;
        self.source_capabilities_refresh_for_contract = false;
        self.source_capabilities_refresh_requested_at_ms = None;
        self.last_request_at_ms = None;
        self.source_capabilities_tx_confirmed = false;
        self.source_capabilities_gcrc_seen = false;
        self.partial_rx_started_at_ms = None;
        self.retry_fail_recovery_pending = false;
    }

    fn interlock_after_vbus_low(&mut self, now_ms: u64) {
        if self.vbus_low_interlocked {
            return;
        }

        // VBUS low is sufficient to withdraw contract authorization and heat,
        // but a static level is not sufficient evidence to withdraw Rd. Keep
        // the physical CC session intact until a transition is confirmed.
        self.clear_contract_authorization(now_ms);
        self.vbus_low_candidate_since_ms = None;
        self.vbus_low_interlocked = true;
    }

    pub(crate) fn interlock_after_stale_contract(&mut self, now_ms: u64) {
        self.clear_contract_authorization(now_ms);
        self.clear_vbus_low_interlock();
        self.awaiting_vbus_restore = false;
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
    }

    pub(crate) fn stale_contract_vin_guard_suspended(&self, now_ms: u64) -> bool {
        matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) || self.last_request_at_ms.is_some_and(|last| {
            now_ms.saturating_sub(last) < FUSB302B_STALE_CONTRACT_VIN_SETTLE_GRACE_MS
        })
    }

    pub(crate) async fn initialize(&mut self, i2c: &mut PdI2c<'_>) -> bool {
        let mut phy = Fusb302::new(&mut *i2c);
        let initialized = phy.init().await.is_ok()
            && phy.pd_reset().await.is_ok()
            && phy.set_host_current_default().await.is_ok()
            && phy.configure_phy(fusb302b_phy_config(false)).await.is_ok()
            && phy.set_cc_pull(CcPin::Cc1, CcPull::Down).await.is_ok()
            && phy.set_cc_pull(CcPin::Cc2, CcPull::Down).await.is_ok()
            && phy.set_measure_cc(None).await.is_ok()
            && phy
                .set_interrupt_masks(FUSB302B_TOGGLE_INTERRUPT_MASKS)
                .await
                .is_ok()
            && phy.read_interrupts().await.is_ok()
            && phy.start_toggle(ToggleMode::Sink).await.is_ok();
        FUSB302B_DIAGNOSTIC.store(
            if initialized {
                FUSB302B_DIAG_WAITING_CC_ATTACH
            } else {
                FUSB302B_DIAG_FAULT
            },
            Ordering::Relaxed,
        );
        initialized
    }

    async fn recover_after_received_reset(
        &mut self,
        i2c: &mut PdI2c<'_>,
        action: Fusb302bReceivedResetAction,
        now: PdTimestamp,
    ) -> bool {
        let now_ms = now.as_millis();
        self.policy.on_received_protocol_reset();
        self.next_message_id = 0;
        self.attached_at_ms = Some(now_ms);
        self.last_source_capabilities_request_at_ms = None;
        self.source_capabilities_refresh_pending = false;
        self.source_capabilities_refresh_for_contract = false;
        self.source_capabilities_refresh_requested_at_ms = None;
        self.last_request_at_ms = None;
        self.source_capabilities_tx_confirmed = false;
        self.source_capabilities_gcrc_seen = false;
        self.partial_rx_started_at_ms = None;
        self.retry_fail_recovery_pending = false;
        self.clear_vbus_low_interlock();
        if !fusb302b_flush_receive_fifo(i2c).await {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_I2C_ERROR, Ordering::Relaxed);
            return true;
        }
        if matches!(
            action,
            Fusb302bReceivedResetAction::AcceptAndWaitForSourceCapabilities
        ) && let Err(fault) = self
            .transmit(i2c, fusb302b::accept_header(self.next_message_id), &[])
            .await
        {
            return self
                .recover_transient_transport_fault(i2c, fault, now)
                .await;
        }
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_SOURCE_CAPS, Ordering::Relaxed);
        true
    }

    async fn recover_after_detach(&mut self, _i2c: &mut PdI2c<'_>) -> bool {
        // `interlock_after_vbus_low` already discarded the contract and kept
        // the policy in bounded discovery. The VBUS transition does not prove
        // that CC detached, so preserve the selected CC/RX PHY session while
        // the source restores power.
        self.vbus_restore_candidate_since_ms = None;
        self.awaiting_vbus_restore = true;

        // Starting Sink toggle here would withdraw Rd during the source's
        // power recovery window and can turn a recoverable VBUS drop into a
        // detach loop.
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_CC_ATTACH, Ordering::Relaxed);
        true
    }

    /// Re-synchronize only the PD protocol engine after VBUS returns. The
    /// FUSB302B driver documents `pd_reset` as a protocol reset; keep the
    /// selected CC pin, Rd termination, and Type-C attachment intact.
    async fn resynchronize_after_vbus_restore(&mut self, i2c: &mut PdI2c<'_>) -> bool {
        let configured = {
            let mut phy = Fusb302::new(&mut *i2c);
            phy.pd_reset().await.is_ok()
                && phy.flush_fifos().await.is_ok()
                && phy.configure_phy(fusb302b_phy_config(true)).await.is_ok()
                && phy
                    .set_interrupt_masks(FUSB302B_RECEIVE_INTERRUPT_MASKS)
                    .await
                    .is_ok()
        };
        if !configured {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_I2C_ERROR, Ordering::Relaxed);
            return false;
        }

        self.next_message_id = 0;
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_SOURCE_CAPS, Ordering::Relaxed);
        true
    }

    /// Recover a local receive/transmit failure without toggling CC. A PHY
    /// reinitialization withdraws Rd briefly and disrupts the active source
    /// attachment.
    async fn recover_transient_transport_fault(
        &mut self,
        i2c: &mut PdI2c<'_>,
        fault: fusb302b::TransientTransportFault,
        now: PdTimestamp,
    ) -> bool {
        let now_ms = now.as_millis();
        match fusb302b::transient_transport_fault_recovery(fault) {
            fusb302b::TransientTransportRecovery::FlushReceiveAndRequery => {
                self.policy.interlock_after_transient_transport_fault();
                self.last_request_at_ms = None;
                // Preserve the normal discovery retry interval after a local
                // fault. RetryFail remains asserted until the next START_TX,
                // so the polling path consumes that already-handled status
                // until this bounded re-query is sent.
                self.last_source_capabilities_request_at_ms = Some(now_ms);
                self.source_capabilities_refresh_pending = false;
                self.source_capabilities_refresh_for_contract = false;
                self.source_capabilities_refresh_requested_at_ms = None;
                self.source_capabilities_tx_confirmed = false;
                self.source_capabilities_gcrc_seen = false;
                self.partial_rx_started_at_ms = None;
                self.retry_fail_recovery_pending =
                    matches!(fault, fusb302b::TransientTransportFault::RetryFailed);
                self.clear_vbus_low_interlock();

                let receive_flushed = fusb302b_flush_receive_fifo(i2c).await;
                let transmit_flushed =
                    !matches!(fault, fusb302b::TransientTransportFault::TransmitIoError)
                        || fusb302b_flush_transmit_fifo(i2c).await;
                if !receive_flushed {
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_I2C_ERROR, Ordering::Relaxed);
                }
                if !transmit_flushed {
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_TX_I2C_ERROR, Ordering::Relaxed);
                }
                FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_SOURCE_CAPS, Ordering::Relaxed);
                true
            }
        }
    }

    pub(crate) fn active_contract(&self) -> Contract {
        self.policy.active_contract()
    }

    pub(crate) fn confirmed_active_contract(&self) -> Option<ConfirmedActiveContract> {
        self.policy.confirmed_active_contract()
    }

    pub(crate) fn source_capabilities(&self) -> Option<SourceCapabilities> {
        self.policy.source_capabilities()
    }

    pub(crate) fn service_available(&self) -> bool {
        !matches!(self.policy.phase(), SinkPhase::Fault)
    }

    pub(crate) async fn request_contract(
        &mut self,
        i2c: &mut PdI2c<'_>,
        request: PdContractRequest,
        now: PdTimestamp,
        replace_pending: bool,
    ) -> PdContractRequestState {
        self.request_rejected = false;
        self.request_timed_out = false;
        let now_ms = now.as_millis();
        let request_inflight = matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        );
        if replace_pending {
            self.policy.cancel_pending_request();
            if !self.policy.prepare_contract_refresh(request) {
                return PdContractRequestState::Failed;
            }
            return self.refresh_source_capabilities(i2c, now, true, true).await;
        }
        if request_inflight {
            if self.policy.pending_contract_matches(request) {
                return PdContractRequestState::Pending;
            }
            self.policy.cancel_pending_request();
            if !self.policy.prepare_contract_refresh(request) {
                return PdContractRequestState::Failed;
            }
            if !fusb302b_flush_receive_fifo(i2c).await {
                self.recover_transient_transport_fault(
                    i2c,
                    fusb302b::TransientTransportFault::ReceiveIoError,
                    now,
                )
                .await;
                return PdContractRequestState::Failed;
            }
            return self.refresh_source_capabilities(i2c, now, true, true).await;
        } else if self
            .policy
            .confirmed_active_contract()
            .is_some_and(|active| request_matches_active(request, Some(active)))
        {
            return PdContractRequestState::Confirmed;
        }
        if self.policy.active_contract().kind == ContractKind::Fixed
            && request.mode() == PdContractRequestMode::Pps
        {
            if !self.policy.prepare_contract_refresh(request) {
                return PdContractRequestState::Failed;
            }
            return self
                .refresh_source_capabilities(i2c, now, false, true)
                .await;
        }
        let Some(rdo) = self.policy.request_contract(request) else {
            return PdContractRequestState::Failed;
        };
        let header = fusb302b::request_header(self.next_message_id);
        if let Err(fault) = self.transmit(i2c, header, &rdo).await {
            let _ = self
                .recover_transient_transport_fault(i2c, fault, now)
                .await;
            return PdContractRequestState::Failed;
        }
        self.last_request_at_ms = Some(now_ms);
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_ACCEPT, Ordering::Relaxed);
        PdContractRequestState::Pending
    }

    pub(crate) async fn refresh_source_capabilities(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now: PdTimestamp,
        replace_pending: bool,
        request_contract: bool,
    ) -> PdContractRequestState {
        self.request_rejected = false;
        self.request_timed_out = false;
        if replace_pending {
            self.source_capabilities_refresh_pending = false;
            self.source_capabilities_refresh_for_contract = false;
            self.source_capabilities_refresh_requested_at_ms = None;
            self.last_source_capabilities_request_at_ms = None;
            self.source_capabilities_tx_confirmed = false;
            self.source_capabilities_gcrc_seen = false;
            self.partial_rx_started_at_ms = None;
            if !fusb302b_flush_receive_fifo(i2c).await {
                self.recover_transient_transport_fault(
                    i2c,
                    fusb302b::TransientTransportFault::ReceiveIoError,
                    now,
                )
                .await;
                return PdContractRequestState::Failed;
            }
        } else if self.source_capabilities_refresh_pending {
            return PdContractRequestState::Pending;
        }
        if self.policy.phase() == SinkPhase::Fault {
            return PdContractRequestState::Failed;
        }
        let now_ms = now.as_millis();
        let header = fusb302b::get_source_capabilities_header(self.next_message_id);
        if let Err(fault) = self.transmit(i2c, header, &[]).await {
            let _ = self
                .recover_transient_transport_fault(i2c, fault, now)
                .await;
            return PdContractRequestState::Failed;
        }
        self.source_capabilities_refresh_pending = true;
        self.source_capabilities_refresh_for_contract = request_contract;
        self.source_capabilities_refresh_requested_at_ms = Some(now_ms);
        self.last_source_capabilities_request_at_ms = Some(now_ms);
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_SOURCE_CAPS_REQUESTED, Ordering::Relaxed);
        PdContractRequestState::Pending
    }

    pub(crate) async fn request_automatic_idle_contract(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now: PdTimestamp,
        replace_pending: bool,
    ) -> PdContractRequestState {
        self.request_rejected = false;
        self.request_timed_out = false;
        let now_ms = now.as_millis();
        if replace_pending {
            self.policy.cancel_pending_request();
            self.policy.prepare_automatic_idle_refresh();
            return self.refresh_source_capabilities(i2c, now, true, true).await;
        }
        if matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) {
            if self.policy.pending_automatic_idle_contract_matches() {
                return PdContractRequestState::Pending;
            }
            self.policy.cancel_pending_request();
        }
        let active = self.policy.active_contract();
        if active.kind == ContractKind::Pps && active.voltage_mv == FUSB302B_INITIAL_PPS_REQUEST_MV
        {
            return PdContractRequestState::Confirmed;
        }
        let Some(rdo) = self.policy.request_automatic_idle_contract() else {
            return PdContractRequestState::Failed;
        };
        let header = fusb302b::request_header(self.next_message_id);
        if let Err(fault) = self.transmit(i2c, header, &rdo).await {
            let _ = self
                .recover_transient_transport_fault(i2c, fault, now)
                .await;
            return PdContractRequestState::Failed;
        }
        self.last_request_at_ms = Some(now_ms);
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_ACCEPT, Ordering::Relaxed);
        PdContractRequestState::Pending
    }

    async fn transmit(
        &mut self,
        i2c: &mut PdI2c<'_>,
        header: u16,
        data: &[u8],
    ) -> Result<(), fusb302b::TransientTransportFault> {
        let Ok(packet) = PdPacket::new(SopType::Sop, header, data) else {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_TX_I2C_ERROR, Ordering::Relaxed);
            return Err(fusb302b::TransientTransportFault::TransmitIoError);
        };
        let mut phy = Fusb302::new(&mut *i2c);
        if phy.transmit(&packet).await.is_err() {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_TX_I2C_ERROR, Ordering::Relaxed);
            return Err(fusb302b::TransientTransportFault::TransmitIoError);
        }
        self.next_message_id = (self.next_message_id + 1) & 0x07;
        Ok(())
    }

    async fn poll_vbus_restore(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now: PdTimestamp,
        now_ms: u64,
    ) -> Option<bool> {
        if !self.awaiting_vbus_restore {
            return None;
        }
        let vbus_restored = {
            let mut phy = Fusb302::new(&mut *i2c);
            match phy.read_status().await {
                Ok(status) => status.status0 & FUSB302B_STATUS0_VBUSOK != 0,
                Err(_) => {
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_I2C_ERROR, Ordering::Relaxed);
                    return Some(
                        self.recover_transient_transport_fault(
                            i2c,
                            fusb302b::TransientTransportFault::ReceiveIoError,
                            now,
                        )
                        .await,
                    );
                }
            }
        };
        if !vbus_restored {
            self.vbus_restore_candidate_since_ms = None;
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_CC_ATTACH, Ordering::Relaxed);
            return Some(true);
        }
        let restore_started = *self.vbus_restore_candidate_since_ms.get_or_insert(now_ms);
        if !fusb302b_vbus_restore_confirmation_expired(Some(restore_started), now_ms) {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
            return Some(true);
        }
        if !self.resynchronize_after_vbus_restore(i2c).await {
            return Some(true);
        }
        self.awaiting_vbus_restore = false;
        self.vbus_restore_candidate_since_ms = None;
        self.vbus_low_candidate_since_ms = None;
        self.vbus_low_interlocked = false;
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
        Some(true)
    }

    async fn poll_pending_request(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now: PdTimestamp,
        now_ms: u64,
    ) -> Option<bool> {
        let pending = matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) && self.last_request_at_ms.is_some_and(|last| {
            now_ms.saturating_sub(last) >= FUSB302B_CONTRACT_REQUEST_TIMEOUT_MS
        });
        if !pending {
            return None;
        }
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_REQUEST_TIMEOUT, Ordering::Relaxed);
        self.request_timed_out = true;
        Some(
            self.recover_transient_transport_fault(
                i2c,
                fusb302b::TransientTransportFault::PendingRequestTimeout,
                now,
            )
            .await,
        )
    }

    fn poll_pps_transition_capability_refresh(&mut self, now_ms: u64) {
        let timed_out = self.source_capabilities_refresh_pending
            && self
                .source_capabilities_refresh_requested_at_ms
                .is_some_and(|last| {
                    now_ms.saturating_sub(last) >= FUSB302B_CONTRACT_REQUEST_TIMEOUT_MS
                });
        if !timed_out {
            return;
        }
        self.source_capabilities_refresh_pending = false;
        self.source_capabilities_refresh_for_contract = false;
        self.source_capabilities_refresh_requested_at_ms = None;
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_REQUEST_TIMEOUT, Ordering::Relaxed);
    }

    async fn poll_attachment(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now: PdTimestamp,
        now_ms: u64,
    ) -> Option<bool> {
        if self.polarity.is_some() {
            return None;
        }
        let polarity = {
            let mut phy = Fusb302::new(&mut *i2c);
            match phy.read_status().await {
                Ok(status) => {
                    fusb302b_settled_sink_polarity(status.status1a).map(|pin| match pin {
                        1 => CcPin::Cc1,
                        2 => CcPin::Cc2,
                        _ => unreachable!(),
                    })
                }
                Err(_) => {
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_I2C_ERROR, Ordering::Relaxed);
                    return Some(
                        self.recover_transient_transport_fault(
                            i2c,
                            fusb302b::TransientTransportFault::ReceiveIoError,
                            now,
                        )
                        .await,
                    );
                }
            }
        };
        let Some(polarity) = polarity else {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_CC_ATTACH, Ordering::Relaxed);
            return Some(true);
        };
        if !self.configure_attached_polarity(i2c, polarity).await {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_I2C_ERROR, Ordering::Relaxed);
            return Some(
                self.recover_transient_transport_fault(
                    i2c,
                    fusb302b::TransientTransportFault::ConfigurationIoError,
                    now,
                )
                .await,
            );
        }
        self.polarity = Some(polarity);
        self.policy.on_attachment_detected();
        self.attached_at_ms = Some(now_ms);
        self.partial_rx_started_at_ms = None;
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_SOURCE_CAPS, Ordering::Relaxed);
        None
    }

    async fn configure_attached_polarity(&mut self, i2c: &mut PdI2c<'_>, polarity: CcPin) -> bool {
        let mut phy = Fusb302::new(&mut *i2c);
        phy.flush_fifos().await.is_ok()
            && phy.stop_toggle().await.is_ok()
            && phy.set_cc_pull(CcPin::Cc1, CcPull::Down).await.is_ok()
            && phy.set_cc_pull(CcPin::Cc2, CcPull::Down).await.is_ok()
            && phy.set_measure_cc(Some(polarity)).await.is_ok()
            && phy.set_tx_cc(polarity).await.is_ok()
            && phy.configure_phy(fusb302b_phy_config(true)).await.is_ok()
            && phy
                .set_interrupt_masks(FUSB302B_RECEIVE_INTERRUPT_MASKS)
                .await
                .is_ok()
    }

    async fn handle_vbus_low_event(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now_ms: u64,
        transition: bool,
    ) -> bool {
        self.interlock_after_vbus_low(now_ms);
        if transition && self.vbus_low_candidate_since_ms.is_none() {
            self.vbus_low_candidate_since_ms = Some(now_ms);
        }
        if fusb302b_vbus_low_confirmation_expired(self.vbus_low_candidate_since_ms, now_ms) {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
            return self.recover_after_detach(i2c).await;
        }
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
        true
    }

    async fn handle_empty_event(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now: PdTimestamp,
        now_ms: u64,
        tx_sent: bool,
        gcrc_sent: bool,
    ) -> bool {
        self.clear_vbus_low_interlock();
        self.partial_rx_started_at_ms = None;
        if self.policy.phase() == SinkPhase::WaitingForSourceCapabilities {
            self.source_capabilities_tx_confirmed |= tx_sent;
            self.source_capabilities_gcrc_seen |= gcrc_sent;
            if !self.source_capabilities_query_due(now_ms) {
                FUSB302B_DIAGNOSTIC.store(self.source_capabilities_diagnostic(), Ordering::Relaxed);
                return true;
            }
            let header = fusb302b::get_source_capabilities_header(self.next_message_id);
            if let Err(fault) = self.transmit(i2c, header, &[]).await {
                return self
                    .recover_transient_transport_fault(i2c, fault, now)
                    .await;
            }
            self.retry_fail_recovery_pending = false;
            self.source_capabilities_tx_confirmed = false;
            self.source_capabilities_gcrc_seen = false;
            self.last_source_capabilities_request_at_ms = Some(now_ms);
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_SOURCE_CAPS_REQUESTED, Ordering::Relaxed);
        } else if self.pps_keepalive_due(now_ms) {
            let Some(rdo) = self.policy.refresh_active_pps() else {
                return self
                    .recover_transient_transport_fault(
                        i2c,
                        fusb302b::TransientTransportFault::ConfigurationIoError,
                        now,
                    )
                    .await;
            };
            let header = fusb302b::request_header(self.next_message_id);
            if let Err(fault) = self.transmit(i2c, header, &rdo).await {
                return self
                    .recover_transient_transport_fault(i2c, fault, now)
                    .await;
            }
            self.last_request_at_ms = Some(now_ms);
        }
        true
    }

    fn source_capabilities_query_due(&self, now_ms: u64) -> bool {
        self.attached_at_ms.is_some_and(|attached_at_ms| {
            fusb302b::source_capabilities_request_due(
                attached_at_ms,
                self.last_source_capabilities_request_at_ms,
                now_ms,
            )
        })
    }

    fn source_capabilities_diagnostic(&self) -> u8 {
        if self.source_capabilities_gcrc_seen {
            FUSB302B_DIAG_SOURCE_CAPS_GCRC_SEEN
        } else if self.source_capabilities_tx_confirmed {
            FUSB302B_DIAG_SOURCE_CAPS_TX_CONFIRMED
        } else if self.last_source_capabilities_request_at_ms.is_some() {
            FUSB302B_DIAG_SOURCE_CAPS_REQUESTED
        } else {
            FUSB302B_DIAG_WAITING_SOURCE_CAPS
        }
    }

    fn pps_keepalive_due(&self, now_ms: u64) -> bool {
        self.policy.phase() == SinkPhase::Ready
            && self.active_contract().kind == ContractKind::Pps
            && self
                .last_request_at_ms
                .is_some_and(|last| fusb302b::pps_keepalive_due(last, now_ms))
    }

    async fn handle_partial_event(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now: PdTimestamp,
        now_ms: u64,
        tx_sent: bool,
        gcrc_sent: bool,
    ) -> bool {
        self.clear_vbus_low_interlock();
        if self.policy.phase() == SinkPhase::WaitingForSourceCapabilities {
            self.source_capabilities_tx_confirmed |= tx_sent;
            self.source_capabilities_gcrc_seen |= gcrc_sent;
        }
        let partial_started_at_ms = self.partial_rx_started_at_ms.get_or_insert(now_ms);
        if now_ms.saturating_sub(*partial_started_at_ms) >= FUSB302B_PARTIAL_RX_TIMEOUT_MS {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
            return self
                .recover_transient_transport_fault(
                    i2c,
                    fusb302b::TransientTransportFault::PartialReceiveTimeout,
                    now,
                )
                .await;
        }
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_PARTIAL, Ordering::Relaxed);
        true
    }

    async fn handle_message_event(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now: PdTimestamp,
        now_ms: u64,
        message: PdPacket,
    ) -> bool {
        self.clear_vbus_low_interlock();
        self.partial_rx_started_at_ms = None;
        if let Some((pdos, count)) =
            fusb302b::source_capabilities_from_message(message.header(), message.payload())
        {
            return self
                .handle_source_capabilities_message(i2c, now, now_ms, message, &pdos[..count])
                .await;
        }
        if message.payload().is_empty() {
            self.handle_control_message(message, now_ms);
        }
        true
    }

    async fn handle_source_capabilities_message(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now: PdTimestamp,
        now_ms: u64,
        message: PdPacket,
        pdos: &[u32],
    ) -> bool {
        let refresh_for_contract = self.source_capabilities_refresh_for_contract;
        let preserve_ready_contract =
            self.policy.phase() == SinkPhase::Ready && !refresh_for_contract;
        self.source_capabilities_refresh_pending = false;
        self.source_capabilities_refresh_for_contract = false;
        self.source_capabilities_refresh_requested_at_ms = None;
        self.source_capabilities_tx_confirmed = false;
        self.source_capabilities_gcrc_seen = false;
        self.retry_fail_recovery_pending = false;
        let message_id = Some((message.header() >> 9) as u8 & 0x07);
        let rdo = if preserve_ready_contract {
            self.policy
                .refresh_source_capabilities_with_message_id(pdos, message_id)
        } else {
            self.policy
                .on_source_capabilities_with_message_id(pdos, message_id)
        };
        if let Some(rdo) = rdo {
            let header = fusb302b::request_header(self.next_message_id);
            if let Err(fault) = self.transmit(i2c, header, &rdo).await {
                return self
                    .recover_transient_transport_fault(i2c, fault, now)
                    .await;
            }
            self.last_request_at_ms = Some(now_ms);
            self.last_source_capabilities_request_at_ms = None;
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_ACCEPT, Ordering::Relaxed);
        } else if self.policy.phase() == SinkPhase::Ready {
            self.last_source_capabilities_request_at_ms = Some(now_ms);
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_IDLE, Ordering::Relaxed);
        } else {
            self.last_source_capabilities_request_at_ms = Some(now_ms);
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_NO_USABLE_CONTRACT, Ordering::Relaxed);
            return true;
        }
        true
    }

    fn handle_control_message(&mut self, message: PdPacket, now_ms: u64) {
        let was_waiting_for_ps_rdy = self.policy.phase() == SinkPhase::WaitingForPsRdy;
        let message_type = (message.header() & 0x1f) as u8;
        self.request_rejected = matches!(message_type, 4 | 12)
            && matches!(
                self.policy.phase(),
                SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
            );
        self.policy.on_control_message_with_message_id(
            message_type,
            Some((message.header() >> 9) as u8 & 0x07),
            now_ms,
        );
        if was_waiting_for_ps_rdy && self.policy.phase() == SinkPhase::Ready {
            self.last_source_capabilities_request_at_ms = Some(now_ms);
        }
        FUSB302B_DIAGNOSTIC.store(
            if self.policy.phase() == SinkPhase::Ready {
                FUSB302B_DIAG_IDLE
            } else {
                FUSB302B_DIAG_WAITING_PS_RDY
            },
            Ordering::Relaxed,
        );
    }

    async fn handle_received_reset(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now: PdTimestamp,
        action: Fusb302bReceivedResetAction,
    ) -> bool {
        self.clear_vbus_low_interlock();
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
        self.recover_after_received_reset(i2c, action, now).await
    }

    async fn handle_retry_failed(&mut self, i2c: &mut PdI2c<'_>, now: PdTimestamp) -> bool {
        self.clear_vbus_low_interlock();
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
        self.recover_transient_transport_fault(
            i2c,
            fusb302b::TransientTransportFault::RetryFailed,
            now,
        )
        .await
    }

    fn handle_protection_event(&mut self) -> bool {
        self.clear_vbus_low_interlock();
        self.policy.mark_fault();
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_PROTECTION, Ordering::Relaxed);
        false
    }

    async fn handle_unsupported_sop(&mut self, i2c: &mut PdI2c<'_>, now: PdTimestamp) -> bool {
        self.clear_vbus_low_interlock();
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_UNSUPPORTED_SOP, Ordering::Relaxed);
        self.recover_transient_transport_fault(
            i2c,
            fusb302b::TransientTransportFault::ReceiveIoError,
            now,
        )
        .await
    }

    /// Drain a bounded number of completed PD frames in one service turn. The
    /// service task has already acquired the bus with a non-blocking try-lock;
    /// awaits below are bounded hardware transactions, not waits for EEPROM's
    /// mutex ownership.
    pub(crate) async fn poll(&mut self, i2c: &mut PdI2c<'_>, now: PdTimestamp) -> bool {
        let now_ms = now.as_millis();
        if self.policy.phase() == SinkPhase::Fault {
            return false;
        }
        if let Some(result) = self.poll_vbus_restore(i2c, now, now_ms).await {
            return result;
        }
        if let Some(result) = self.poll_pending_request(i2c, now, now_ms).await {
            return result;
        }
        self.poll_pps_transition_capability_refresh(now_ms);
        if let Some(result) = self.poll_attachment(i2c, now, now_ms).await {
            return result;
        }
        self.poll_receive_messages(i2c, now, now_ms).await
    }

    async fn poll_receive_messages(
        &mut self,
        i2c: &mut PdI2c<'_>,
        now: PdTimestamp,
        now_ms: u64,
    ) -> bool {
        let event = match fusb302b_receive_event(i2c, self.retry_fail_recovery_pending).await {
            Ok(event) => event,
            Err(fault) => {
                FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_I2C_ERROR, Ordering::Relaxed);
                return self
                    .recover_transient_transport_fault(i2c, fault, now)
                    .await;
            }
        };
        match event {
            Fusb302bReceiveEvent::VbusLow { transition } => {
                self.handle_vbus_low_event(i2c, now_ms, transition).await
            }
            Fusb302bReceiveEvent::Empty { tx_sent, gcrc_sent } => {
                self.handle_empty_event(i2c, now, now_ms, tx_sent, gcrc_sent)
                    .await
            }
            Fusb302bReceiveEvent::Partial { tx_sent, gcrc_sent } => {
                self.handle_partial_event(i2c, now, now_ms, tx_sent, gcrc_sent)
                    .await
            }
            Fusb302bReceiveEvent::Message(message) => {
                self.handle_message_event(i2c, now, now_ms, message).await
            }
            Fusb302bReceiveEvent::ReceivedReset(action) => {
                self.handle_received_reset(i2c, now, action).await
            }
            Fusb302bReceiveEvent::RetryFailed => self.handle_retry_failed(i2c, now).await,
            Fusb302bReceiveEvent::Protection => self.handle_protection_event(),
            Fusb302bReceiveEvent::UnsupportedSop => self.handle_unsupported_sop(i2c, now).await,
        }
    }
}
