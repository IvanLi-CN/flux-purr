#[cfg(any(target_arch = "xtensa", test))]
fn vin_adc_mv_for_input_mv(input_mv: u32) -> u16 {
    let numerator = input_mv.saturating_mul(s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS);
    let denominator =
        s3_frontpanel::VIN_DIVIDER_R_HIGH_OHMS + s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS;
    (numerator / denominator).min(u32::from(u16::MAX)) as u16
}

#[cfg(target_arch = "xtensa")]
fn vin_input_mv_from_adc_mv(adc_mv: u16) -> u32 {
    let numerator = u32::from(adc_mv).saturating_mul(
        s3_frontpanel::VIN_DIVIDER_R_HIGH_OHMS + s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS,
    );
    numerator / s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS
}

#[cfg(any(target_arch = "xtensa", test))]
fn rtd_fractional_mean_mv(sum_mv: u32, valid_samples: usize) -> Option<f32> {
    if valid_samples < RTD_MIN_VALID_SAMPLE_COUNT {
        return None;
    }
    Some(sum_mv as f32 / valid_samples as f32)
}

#[cfg(test)]
fn oversampled_fractional_mean_mv_with_discard<F>(
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
fn oversampled_rtd_batch_with_discard<F>(
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
fn phase_averaged_rtd_batch_with_discard<F, W>(
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
const fn mask_adc1_raw_code(value: u16) -> u16 {
    value & 0x0fff
}

#[cfg(target_arch = "xtensa")]
type Adc1Driver = Adc<'static, esp_hal::peripherals::ADC1<'static>, esp_hal::Blocking>;
#[cfg(target_arch = "xtensa")]
type VinAdcPin = esp_hal::analog::adc::AdcPin<
    esp_hal::peripherals::GPIO1<'static>,
    esp_hal::peripherals::ADC1<'static>,
    AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
>;
#[cfg(target_arch = "xtensa")]
type RtdAdcPin = esp_hal::analog::adc::AdcPin<
    esp_hal::peripherals::GPIO2<'static>,
    esp_hal::peripherals::ADC1<'static>,
    AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
>;
#[cfg(target_arch = "xtensa")]
type Adc1Curve = AdcCalCurve<esp_hal::peripherals::ADC1<'static>>;

#[cfg(target_arch = "xtensa")]
fn initialize_adc1(
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
struct PdAdcService<'a, 'i, PWM> {
    i2c: &'a mut I2c<'i, esp_hal::Blocking>,
    pd_port: &'a mut PdPort,
    last_pd_observation: &'a mut Option<PdStatusObservation>,
    heater_pwm: &'a mut PWM,
    last_heater_duty: &'a mut u8,
}

#[cfg(target_arch = "xtensa")]
async fn service_pd_for_adc<PWM>(service: &mut PdAdcService<'_, '_, PWM>)
where
    PWM: SetDutyCycle,
{
    let observation = read_pd_status(service.i2c, service.pd_port, PdTimestamp::now()).await;
    *service.last_pd_observation = observation;
    if !startup_pd_contract_ready(observation) {
        // ADC acquisition can outlive a normal control turn. Cut the physical
        // output at the same service boundary instead of waiting for sampling
        // to finish before the main loop applies its state reconciliation.
        apply_heater_duty(service.heater_pwm, 0, service.last_heater_duty);
    }
}

#[cfg(target_arch = "xtensa")]
async fn read_adc_sample_with_pd<PIN, PWM>(
    adc: &mut Adc1Driver,
    pin: &mut esp_hal::analog::adc::AdcPin<
        PIN,
        esp_hal::peripherals::ADC1<'static>,
        AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
    >,
    curve: &Adc1Curve,
    service: &mut PdAdcService<'_, '_, PWM>,
    last_pd_service_ms: &mut u64,
) -> Option<AdcConvertedSample>
where
    PIN: AdcChannel,
    PWM: SetDutyCycle,
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
            Err(nb::Error::WouldBlock) => {
                let elapsed_ms = Instant::now()
                    .as_millis()
                    .saturating_sub(*last_pd_service_ms);
                if elapsed_ms >= PD_RUNTIME_SERVICE_INTERVAL_MS {
                    service_pd_for_adc(service).await;
                    *last_pd_service_ms = Instant::now().as_millis();
                }
                EmbassyTimer::after_micros(50).await;
            }
            Err(_) => return None,
        }
    }
}

#[cfg(target_arch = "xtensa")]
async fn wait_for_adc_phase_if_needed(should_wait: bool) {
    if should_wait {
        EmbassyTimer::after_micros(u64::from(RTD_SAMPLE_PWM_PHASE_SPACING_US)).await;
    }
}

#[cfg(target_arch = "xtensa")]
async fn read_adc_batch_with_pd<PIN, PWM>(
    adc: &mut Adc1Driver,
    pin: &mut esp_hal::analog::adc::AdcPin<
        PIN,
        esp_hal::peripherals::ADC1<'static>,
        AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
    >,
    curve: &Adc1Curve,
    service: &mut PdAdcService<'_, '_, PWM>,
) -> Option<RtdAdcBatch>
where
    PIN: AdcChannel,
    PWM: SetDutyCycle,
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
    let mut samples_since_pd = 0_usize;
    let mut last_pd_service_ms = Instant::now().as_millis();

    // Keep the calibrated acquisition plan intact while yielding between
    // conversions. The previous synchronous implementation could monopolize
    // the executor across the whole discard and phase-averaging batch.
    let settle_deadline = Instant::now().checked_add(Duration::from_micros(u64::from(
        RTD_CHANNEL_SWITCH_SETTLE_US,
    )))?;
    while Instant::now() < settle_deadline {
        service_pd_for_adc(service).await;
        last_pd_service_ms = Instant::now().as_millis();
        EmbassyTimer::after_millis(1).await;
    }

    for _ in 0..total_samples {
        let sample = read_adc_sample_with_pd(
            adc,
            pin,
            curve,
            service,
            &mut last_pd_service_ms,
        )
        .await;

        samples_since_pd = samples_since_pd.saturating_add(1);
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

        if samples_since_pd >= ADC_PD_SERVICE_MAX_SAMPLES
            || Instant::now()
                .as_millis()
                .saturating_sub(last_pd_service_ms)
                >= PD_RUNTIME_SERVICE_INTERVAL_MS
        {
            service_pd_for_adc(service).await;
            samples_since_pd = 0;
            last_pd_service_ms = Instant::now().as_millis();
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
async fn read_calibrated_vin_mv_with_pd<PWM: SetDutyCycle>(
    adc: &mut Adc1Driver,
    pin: &mut VinAdcPin,
    curve: Option<&Adc1Curve>,
    memory_config: &MemoryConfig,
    service: &mut PdAdcService<'_, '_, PWM>,
) -> Option<(u16, u16, u16, u32)> {
    let curve = curve?;
    let batch = read_adc_batch_with_pd(adc, pin, curve, service).await?;
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
async fn read_rtd_sample_with_pd<PWM: SetDutyCycle>(
    adc: &mut Adc1Driver,
    pin: &mut RtdAdcPin,
    curve: Option<&Adc1Curve>,
    memory_config: &MemoryConfig,
    service: &mut PdAdcService<'_, '_, PWM>,
) -> RtdSample {
    let Some(curve) = curve else {
        return RtdSample::Fault {
            adc_mv: None,
            reason: HeaterFaultReason::AdcReadFailed,
        };
    };
    let Some(batch) = read_adc_batch_with_pd(adc, pin, curve, service).await else {
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
const FUSB302B_STATUS0_CRC_CHECK: u8 = 1 << 4;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_STATUS0A_RETRY_FAIL: u8 = 1 << 4;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_STATUS0_VBUSOK: u8 = 1 << 7;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_STATUS1_RX_EMPTY: u8 = 1 << 5;
#[cfg(target_arch = "xtensa")]
const FUSB302B_STATUS1_OVERTEMP: u8 = 1 << 1;
#[cfg(target_arch = "xtensa")]
const FUSB302B_STATUS1_VCONN_OCP: u8 = 1;
#[cfg(target_arch = "xtensa")]
const FUSB302B_STATUS1A_RXSOP: u8 = 1;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_TOGSS_MASK: u8 = 0b0011_1000;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_TOGSS_SNK_CC1: u8 = 0b0010_1000;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_TOGSS_SNK_CC2: u8 = 0b0011_0000;
#[cfg(target_arch = "xtensa")]
const FUSB302B_INTERRUPTA_TX_SENT: u8 = 1 << 2;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_INTERRUPT_VBUSOK: u8 = 1 << 7;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_VBUS_LOW_CONFIRM_MS: u64 = 50;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_VBUS_RESTORE_CONFIRM_MS: u64 = 50;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_INTERRUPTA_SOFT_RESET: u8 = 1 << 1;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_INTERRUPTA_HARD_RESET: u8 = 1;
#[cfg(target_arch = "xtensa")]
const FUSB302B_INTERRUPTB_GCRC_SENT: u8 = 1;
#[cfg(target_arch = "xtensa")]
const FUSB302B_CONTROL1_REGISTER: u8 = 0x07;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_CONTROL1_RW_MASK: u8 = 0b0111_0011;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_CONTROL1_RX_FLUSH: u8 = 1 << 2;
#[cfg(target_arch = "xtensa")]
const FUSB302B_CONTROL0_REGISTER: u8 = 0x06;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_CONTROL0_RW_MASK: u8 = 0b0010_1110;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_CONTROL0_TX_FLUSH: u8 = 1 << 6;
#[cfg(target_arch = "xtensa")]
const FUSB302B_TOGGLE_INTERRUPT_MASKS: InterruptMasks = InterruptMasks::new(0x7f, 0xbf, 0xff);
#[cfg(target_arch = "xtensa")]
const FUSB302B_RECEIVE_INTERRUPT_MASKS: InterruptMasks = InterruptMasks::new(0x7d, 0xe0, 0x00);

#[cfg(any(target_arch = "xtensa", test))]
const fn fusb302b_settled_sink_polarity(status1a: u8) -> Option<u8> {
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
const fn fusb302b_vbus_detach_was_reported(interrupt: u8, status0: u8) -> bool {
    interrupt & FUSB302B_INTERRUPT_VBUSOK != 0 && status0 & FUSB302B_STATUS0_VBUSOK == 0
}

#[cfg(any(target_arch = "xtensa", test))]
const fn fusb302b_vbus_low_confirmation_expired(
    candidate_since_ms: Option<u64>,
    now_ms: u64,
) -> bool {
    match candidate_since_ms {
        Some(started) => now_ms.saturating_sub(started) >= FUSB302B_VBUS_LOW_CONFIRM_MS,
        None => false,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
const fn fusb302b_vbus_restore_confirmation_expired(
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
enum Fusb302bReceiveEvent {
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
enum Fusb302bReceivedResetAction {
    AcceptAndWaitForSourceCapabilities,
    WaitForSourceCapabilities,
}

#[cfg(target_arch = "xtensa")]
const fn fusb302b_phy_config(auto_goodcrc: bool) -> PhyConfig {
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
const fn fusb302b_received_reset_action(interrupt_a: u8) -> Option<Fusb302bReceivedResetAction> {
    if interrupt_a & FUSB302B_INTERRUPTA_HARD_RESET != 0 {
        Some(Fusb302bReceivedResetAction::WaitForSourceCapabilities)
    } else if interrupt_a & FUSB302B_INTERRUPTA_SOFT_RESET != 0 {
        Some(Fusb302bReceivedResetAction::AcceptAndWaitForSourceCapabilities)
    } else {
        None
    }
}

#[cfg(any(target_arch = "xtensa", test))]
const fn fusb302b_receive_fifo_flush_value(control1: u8) -> u8 {
    (control1 & FUSB302B_CONTROL1_RW_MASK) | FUSB302B_CONTROL1_RX_FLUSH
}

#[cfg(any(target_arch = "xtensa", test))]
const fn fusb302b_transmit_fifo_flush_value(control0: u8) -> u8 {
    (control0 & FUSB302B_CONTROL0_RW_MASK) | FUSB302B_CONTROL0_TX_FLUSH
}

#[cfg(any(target_arch = "xtensa", test))]
const fn fusb302b_retry_failure_requires_recovery(
    status0a: u8,
    _status1: u8,
    retry_fail_recovery_pending: bool,
) -> bool {
    !retry_fail_recovery_pending && status0a & FUSB302B_STATUS0A_RETRY_FAIL != 0
}

#[cfg(any(target_arch = "xtensa", test))]
const fn fusb302b_retry_recovery_should_discard_frame(
    status1: u8,
    retry_fail_recovery_pending: bool,
) -> bool {
    retry_fail_recovery_pending && status1 & FUSB302B_STATUS1_RX_EMPTY == 0
}

/// The upstream PHY API exposes only a combined FIFO flush. Receive recovery
/// therefore updates only CONTROL1.RX_FLUSH and preserves the driver's
/// receive-mask bits; transmit recovery uses the separate TX flush below.
#[cfg(target_arch = "xtensa")]
fn fusb302b_flush_receive_fifo(i2c: &mut I2c<'_, esp_hal::Blocking>) -> bool {
    let mut control1 = [0_u8];
    i2c.write_read(
        fusb302::DEFAULT_ADDRESS,
        &[FUSB302B_CONTROL1_REGISTER],
        &mut control1,
    )
    .is_ok()
        && i2c
            .write(
                fusb302::DEFAULT_ADDRESS,
                &[
                    FUSB302B_CONTROL1_REGISTER,
                    fusb302b_receive_fifo_flush_value(control1[0]),
                ],
            )
            .is_ok()
}

#[cfg(target_arch = "xtensa")]
fn fusb302b_flush_transmit_fifo(i2c: &mut I2c<'_, esp_hal::Blocking>) -> bool {
    let mut control0 = [0_u8];
    i2c.write_read(
        fusb302::DEFAULT_ADDRESS,
        &[FUSB302B_CONTROL0_REGISTER],
        &mut control0,
    )
    .is_ok()
        && i2c
            .write(
                fusb302::DEFAULT_ADDRESS,
                &[
                    FUSB302B_CONTROL0_REGISTER,
                    fusb302b_transmit_fifo_flush_value(control0[0]),
                ],
            )
            .is_ok()
}

#[cfg(target_arch = "xtensa")]
struct Fusb302bRuntime {
    policy: fusb302b::SinkPolicy,
    polarity: Option<CcPin>,
    next_message_id: u8,
    attached_at_ms: Option<u64>,
    last_source_capabilities_request_at_ms: Option<u64>,
    source_capabilities_refresh_pending: bool,
    source_capabilities_refresh_requested_at_ms: Option<u64>,
    source_capabilities_refresh_kind: Option<SourceCapabilitiesRefreshKind>,
    last_request_at_ms: Option<u64>,
    source_capabilities_tx_confirmed: bool,
    source_capabilities_gcrc_seen: bool,
    partial_rx_started_at_ms: Option<u64>,
    retry_fail_recovery_pending: bool,
    vbus_low_candidate_since_ms: Option<u64>,
    vbus_low_interlocked: bool,
    vbus_restore_candidate_since_ms: Option<u64>,
    awaiting_vbus_restore: bool,
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PdContractRequestState {
    Confirmed,
    Pending,
    Failed,
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceCapabilitiesRefreshKind {
    PpsTransition,
    FixedContractLiveness,
}

#[cfg(target_arch = "xtensa")]
impl Fusb302bRuntime {
    const fn new() -> Self {
        Self {
            policy: fusb302b::SinkPolicy::new(
                FUSB302B_INITIAL_PPS_REQUEST_MV,
                MAX_HEATER_CONTRACT_MA,
            ),
            polarity: None,
            next_message_id: 0,
            attached_at_ms: None,
            last_source_capabilities_request_at_ms: None,
            source_capabilities_refresh_pending: false,
            source_capabilities_refresh_requested_at_ms: None,
            source_capabilities_refresh_kind: None,
            last_request_at_ms: None,
            source_capabilities_tx_confirmed: false,
            source_capabilities_gcrc_seen: false,
            partial_rx_started_at_ms: None,
            retry_fail_recovery_pending: false,
            vbus_low_candidate_since_ms: None,
            vbus_low_interlocked: false,
            vbus_restore_candidate_since_ms: None,
            awaiting_vbus_restore: false,
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
        self.source_capabilities_refresh_requested_at_ms = None;
        self.source_capabilities_refresh_kind = None;
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

    fn interlock_after_stale_contract(&mut self, now_ms: u64) {
        self.clear_contract_authorization(now_ms);
        self.clear_vbus_low_interlock();
        self.awaiting_vbus_restore = false;
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
    }

    fn stale_contract_vin_guard_suspended(&self, now_ms: u64) -> bool {
        matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) || self.last_request_at_ms.is_some_and(|last| {
            now_ms.saturating_sub(last) < FUSB302B_STALE_CONTRACT_VIN_SETTLE_GRACE_MS
        })
    }

    async fn initialize(&mut self, i2c: &mut I2c<'_, esp_hal::Blocking>) -> bool {
        let mut phy = Fusb302::new(BlockingAsync::new(i2c));
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
        i2c: &mut I2c<'_, esp_hal::Blocking>,
        action: Fusb302bReceivedResetAction,
        now: PdTimestamp,
    ) -> bool {
        let now_ms = now.as_millis();
        self.policy.on_received_protocol_reset();
        self.next_message_id = 0;
        self.attached_at_ms = Some(now_ms);
        self.last_source_capabilities_request_at_ms = None;
        self.source_capabilities_refresh_pending = false;
        self.source_capabilities_refresh_requested_at_ms = None;
        self.source_capabilities_refresh_kind = None;
        self.last_request_at_ms = None;
        self.source_capabilities_tx_confirmed = false;
        self.source_capabilities_gcrc_seen = false;
        self.partial_rx_started_at_ms = None;
        self.retry_fail_recovery_pending = false;
        self.clear_vbus_low_interlock();
        if !fusb302b_flush_receive_fifo(i2c) {
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

    async fn recover_after_detach(&mut self, _i2c: &mut I2c<'_, esp_hal::Blocking>) -> bool {
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
    async fn resynchronize_after_vbus_restore(
        &mut self,
        i2c: &mut I2c<'_, esp_hal::Blocking>,
    ) -> bool {
        let configured = {
            let mut phy = Fusb302::new(BlockingAsync::new(&mut *i2c));
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
        i2c: &mut I2c<'_, esp_hal::Blocking>,
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
                self.source_capabilities_refresh_requested_at_ms = None;
                self.source_capabilities_refresh_kind = None;
                self.source_capabilities_tx_confirmed = false;
                self.source_capabilities_gcrc_seen = false;
                self.partial_rx_started_at_ms = None;
                self.retry_fail_recovery_pending =
                    matches!(fault, fusb302b::TransientTransportFault::RetryFailed);
                self.clear_vbus_low_interlock();

                let receive_flushed = fusb302b_flush_receive_fifo(i2c);
                let transmit_flushed =
                    !matches!(fault, fusb302b::TransientTransportFault::TransmitIoError)
                        || fusb302b_flush_transmit_fifo(i2c);
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

    fn active_contract(&self) -> Contract {
        self.policy.active_contract()
    }

    fn source_capabilities(&self) -> Option<SourceCapabilities> {
        self.policy.source_capabilities()
    }

    async fn request_pps_voltage(
        &mut self,
        i2c: &mut I2c<'_, esp_hal::Blocking>,
        requested_mv: u16,
        now: PdTimestamp,
    ) -> PdContractRequestState {
        let now_ms = now.as_millis();
        let active = self.policy.active_contract();
        if active.kind == ContractKind::Pps && active.voltage_mv == requested_mv {
            return PdContractRequestState::Confirmed;
        }
        if matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) {
            return PdContractRequestState::Pending;
        }
        if active.kind == ContractKind::Fixed {
            if self.source_capabilities_refresh_pending {
                return PdContractRequestState::Pending;
            }
            if !self.policy.prepare_pps_request(requested_mv) {
                return PdContractRequestState::Failed;
            }
            let header = fusb302b::get_source_capabilities_header(self.next_message_id);
            if let Err(fault) = self.transmit(i2c, header, &[]).await {
                let _ = self
                    .recover_transient_transport_fault(i2c, fault, now)
                    .await;
                return PdContractRequestState::Failed;
            }
            self.source_capabilities_refresh_pending = true;
            self.source_capabilities_refresh_requested_at_ms = Some(now_ms);
            self.source_capabilities_refresh_kind =
                Some(SourceCapabilitiesRefreshKind::PpsTransition);
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_SOURCE_CAPS_REQUESTED, Ordering::Relaxed);
            return PdContractRequestState::Pending;
        }
        let Some(rdo) = self.policy.request_pps_voltage(requested_mv) else {
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

    async fn request_fixed_voltage(
        &mut self,
        i2c: &mut I2c<'_, esp_hal::Blocking>,
        requested_mv: u16,
        now: PdTimestamp,
    ) -> PdContractRequestState {
        let now_ms = now.as_millis();
        let active = self.policy.active_contract();
        if active.kind == ContractKind::Fixed && active.voltage_mv == requested_mv {
            return PdContractRequestState::Confirmed;
        }
        if matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) {
            return PdContractRequestState::Pending;
        }
        let Some(rdo) = self.policy.request_fixed_voltage(requested_mv) else {
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
        i2c: &mut I2c<'_, esp_hal::Blocking>,
        header: u16,
        data: &[u8],
    ) -> Result<(), fusb302b::TransientTransportFault> {
        let Ok(packet) = PdPacket::new(SopType::Sop, header, data) else {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_TX_I2C_ERROR, Ordering::Relaxed);
            return Err(fusb302b::TransientTransportFault::TransmitIoError);
        };
        let mut phy = Fusb302::new(BlockingAsync::new(i2c));
        if phy.transmit(&packet).await.is_err() {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_TX_I2C_ERROR, Ordering::Relaxed);
            return Err(fusb302b::TransientTransportFault::TransmitIoError);
        }
        self.next_message_id = (self.next_message_id + 1) & 0x07;
        Ok(())
    }

    /// Drain a bounded number of completed PD frames in one service turn. No
    /// call awaits while I2C is borrowed, so EEPROM traffic remains independent
    /// of the controller's PD timing.
    #[expect(
        clippy::too_many_lines,
        clippy::excessive_nesting,
        reason = "legacy workflow preserves protocol ordering and safety checks"
    )]
    async fn poll(&mut self, i2c: &mut I2c<'_, esp_hal::Blocking>, now: PdTimestamp) -> bool {
        let now_ms = now.as_millis();
        if self.policy.phase() == SinkPhase::Fault {
            return false;
        }

        if self.awaiting_vbus_restore {
            let vbus_restored = {
                let mut phy = Fusb302::new(BlockingAsync::new(&mut *i2c));
                match phy.read_status().await {
                    Ok(status) => status.status0 & FUSB302B_STATUS0_VBUSOK != 0,
                    Err(_) => {
                        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_I2C_ERROR, Ordering::Relaxed);
                        return self
                            .recover_transient_transport_fault(
                                i2c,
                                fusb302b::TransientTransportFault::ReceiveIoError,
                                now,
                            )
                            .await;
                    }
                }
            };
            if !vbus_restored {
                self.vbus_restore_candidate_since_ms = None;
                FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_CC_ATTACH, Ordering::Relaxed);
                return true;
            }

            let restore_started = *self.vbus_restore_candidate_since_ms.get_or_insert(now_ms);
            if !fusb302b_vbus_restore_confirmation_expired(Some(restore_started), now_ms) {
                FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
                return true;
            }

            // VBUS is present again. Reset only the PD protocol engine so the
            // source and sink restart message-ID state in sync. Reinitializing
            // the whole PHY or restarting CC would withdraw Rd and can make
            // the source remove VBUS again.
            if !self.resynchronize_after_vbus_restore(i2c).await {
                return true;
            }
            self.awaiting_vbus_restore = false;
            self.vbus_restore_candidate_since_ms = None;
            self.vbus_low_candidate_since_ms = None;
            self.vbus_low_interlocked = false;
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
            return true;
        }

        if matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) && self
            .last_request_at_ms
            .is_some_and(|last| now_ms.saturating_sub(last) >= FUSB302B_CONTRACT_REQUEST_TIMEOUT_MS)
        {
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_REQUEST_TIMEOUT, Ordering::Relaxed);
            return self
                .recover_transient_transport_fault(
                    i2c,
                    fusb302b::TransientTransportFault::PendingRequestTimeout,
                    now,
                )
                .await;
        }

        if self.source_capabilities_refresh_pending
            && self
                .source_capabilities_refresh_requested_at_ms
                .is_some_and(|last| {
                    now_ms.saturating_sub(last) >= FUSB302B_CONTRACT_REQUEST_TIMEOUT_MS
                })
        {
            let refresh_kind = self.source_capabilities_refresh_kind;
            self.source_capabilities_refresh_pending = false;
            self.source_capabilities_refresh_requested_at_ms = None;
            self.source_capabilities_refresh_kind = None;
            if refresh_kind == Some(SourceCapabilitiesRefreshKind::FixedContractLiveness) {
                // A fixed contract has no PPS keepalive. A bounded unanswered
                // Source_Capabilities probe is the protocol-level evidence
                // available while the MCU remains powered; interlock heat and
                // re-enter discovery without changing CC termination.
                FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_REQUEST_TIMEOUT, Ordering::Relaxed);
                return self
                    .recover_transient_transport_fault(
                        i2c,
                        fusb302b::TransientTransportFault::PendingRequestTimeout,
                        now,
                    )
                    .await;
            }
            // A failed PPS capability refresh does not invalidate the active
            // fixed contract. Leave it usable and retry only on a later
            // explicit PPS request.
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_REQUEST_TIMEOUT, Ordering::Relaxed);
        }

        if fixed_contract_liveness_probe_due(
            self.active_contract().kind,
            self.policy.phase() == SinkPhase::Ready,
            self.source_capabilities_refresh_pending,
            self.last_source_capabilities_request_at_ms,
            now_ms,
        ) {
            let header = fusb302b::get_source_capabilities_header(self.next_message_id);
            if let Err(fault) = self.transmit(i2c, header, &[]).await {
                return self
                    .recover_transient_transport_fault(i2c, fault, now)
                    .await;
            }
            self.source_capabilities_refresh_pending = true;
            self.source_capabilities_refresh_requested_at_ms = Some(now_ms);
            self.source_capabilities_refresh_kind =
                Some(SourceCapabilitiesRefreshKind::FixedContractLiveness);
            self.last_source_capabilities_request_at_ms = Some(now_ms);
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_SOURCE_CAPS_REQUESTED, Ordering::Relaxed);
        }

        if self.polarity.is_none() {
            let polarity = {
                let mut phy = Fusb302::new(BlockingAsync::new(&mut *i2c));
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
                        return self
                            .recover_transient_transport_fault(
                                i2c,
                                fusb302b::TransientTransportFault::ReceiveIoError,
                                now,
                            )
                            .await;
                    }
                }
            };
            if let Some(polarity) = polarity {
                let selected = {
                    let mut phy = Fusb302::new(BlockingAsync::new(&mut *i2c));
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
                };
                if !selected {
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_I2C_ERROR, Ordering::Relaxed);
                    return self
                        .recover_transient_transport_fault(
                            i2c,
                            fusb302b::TransientTransportFault::ConfigurationIoError,
                            now,
                        )
                        .await;
                }
                self.polarity = Some(polarity);
                self.policy.on_attachment_detected();
                self.attached_at_ms = Some(now_ms);
                self.partial_rx_started_at_ms = None;
                FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_SOURCE_CAPS, Ordering::Relaxed);
            } else {
                FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_CC_ATTACH, Ordering::Relaxed);
                return true;
            }
        }

        for _ in 0..FUSB302B_MAX_RX_MESSAGES_PER_POLL {
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
                    self.interlock_after_vbus_low(now_ms);
                    if transition && self.vbus_low_candidate_since_ms.is_none() {
                        self.vbus_low_candidate_since_ms = Some(now_ms);
                    }
                    if fusb302b_vbus_low_confirmation_expired(
                        self.vbus_low_candidate_since_ms,
                        now_ms,
                    ) {
                        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
                        return self.recover_after_detach(i2c).await;
                    }
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
                    return true;
                }
                Fusb302bReceiveEvent::Empty { tx_sent, gcrc_sent } => {
                    self.clear_vbus_low_interlock();
                    self.partial_rx_started_at_ms = None;
                    if self.policy.phase() == SinkPhase::WaitingForSourceCapabilities {
                        self.source_capabilities_tx_confirmed |= tx_sent;
                        self.source_capabilities_gcrc_seen |= gcrc_sent;
                        let query_due = self.attached_at_ms.is_some_and(|attached_at_ms| {
                            fusb302b::source_capabilities_request_due(
                                attached_at_ms,
                                self.last_source_capabilities_request_at_ms,
                                now_ms,
                            )
                        });
                        if !query_due {
                            let diagnostic = if self.source_capabilities_gcrc_seen {
                                FUSB302B_DIAG_SOURCE_CAPS_GCRC_SEEN
                            } else if self.source_capabilities_tx_confirmed {
                                FUSB302B_DIAG_SOURCE_CAPS_TX_CONFIRMED
                            } else if self.last_source_capabilities_request_at_ms.is_some() {
                                FUSB302B_DIAG_SOURCE_CAPS_REQUESTED
                            } else {
                                FUSB302B_DIAG_WAITING_SOURCE_CAPS
                            };
                            FUSB302B_DIAGNOSTIC.store(diagnostic, Ordering::Relaxed);
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
                        FUSB302B_DIAGNOSTIC
                            .store(FUSB302B_DIAG_SOURCE_CAPS_REQUESTED, Ordering::Relaxed);
                    } else if self.policy.phase() == SinkPhase::Ready
                        && self.active_contract().kind == ContractKind::Pps
                        && self
                            .last_request_at_ms
                            .is_some_and(|last| fusb302b::pps_keepalive_due(last, now_ms))
                    {
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
                    return true;
                }
                Fusb302bReceiveEvent::Partial { tx_sent, gcrc_sent } => {
                    self.clear_vbus_low_interlock();
                    if self.policy.phase() == SinkPhase::WaitingForSourceCapabilities {
                        self.source_capabilities_tx_confirmed |= tx_sent;
                        self.source_capabilities_gcrc_seen |= gcrc_sent;
                    }
                    let partial_started_at_ms = self.partial_rx_started_at_ms.get_or_insert(now_ms);
                    if now_ms.saturating_sub(*partial_started_at_ms)
                        >= FUSB302B_PARTIAL_RX_TIMEOUT_MS
                    {
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
                    return true;
                }
                Fusb302bReceiveEvent::Message(message) => {
                    self.clear_vbus_low_interlock();
                    self.partial_rx_started_at_ms = None;
                    if let Some((pdos, count)) = fusb302b::source_capabilities_from_message(
                        message.header(),
                        message.payload(),
                    ) {
                        let refresh_kind = self.source_capabilities_refresh_kind;
                        let preserve_ready_contract = self.policy.phase() == SinkPhase::Ready
                            && refresh_kind != Some(SourceCapabilitiesRefreshKind::PpsTransition);
                        self.source_capabilities_refresh_pending = false;
                        self.source_capabilities_refresh_requested_at_ms = None;
                        self.source_capabilities_refresh_kind = None;
                        self.source_capabilities_tx_confirmed = false;
                        self.source_capabilities_gcrc_seen = false;
                        self.retry_fail_recovery_pending = false;
                        let rdo = if preserve_ready_contract {
                            self.policy.refresh_source_capabilities_with_message_id(
                                &pdos[..count],
                                Some((message.header() >> 9) as u8 & 0x07),
                            )
                        } else {
                            self.policy.on_source_capabilities_with_message_id(
                                &pdos[..count],
                                Some((message.header() >> 9) as u8 & 0x07),
                            )
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
                            FUSB302B_DIAGNOSTIC
                                .store(FUSB302B_DIAG_WAITING_ACCEPT, Ordering::Relaxed);
                        } else if self.policy.phase() == SinkPhase::Ready {
                            // A liveness response refreshed the cache while
                            // keeping the explicit contract intact. Schedule
                            // the next probe from this confirmed response.
                            self.last_source_capabilities_request_at_ms = Some(now_ms);
                            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_IDLE, Ordering::Relaxed);
                        } else {
                            // The attachment is still valid, but this source
                            // cannot satisfy the heater contract. Keep the
                            // policy in discovery so a later capability update
                            // can recover without a CC reset.
                            self.last_source_capabilities_request_at_ms = Some(now_ms);
                            FUSB302B_DIAGNOSTIC
                                .store(FUSB302B_DIAG_NO_USABLE_CONTRACT, Ordering::Relaxed);
                            return true;
                        }
                    } else if message.payload().is_empty() {
                        let was_waiting_for_ps_rdy =
                            self.policy.phase() == SinkPhase::WaitingForPsRdy;
                        self.policy.on_control_message_with_message_id(
                            (message.header() & 0x1f) as u8,
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
                }
                Fusb302bReceiveEvent::ReceivedReset(action) => {
                    self.clear_vbus_low_interlock();
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
                    return self.recover_after_received_reset(i2c, action, now).await;
                }
                Fusb302bReceiveEvent::RetryFailed => {
                    self.clear_vbus_low_interlock();
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
                    return self
                        .recover_transient_transport_fault(
                            i2c,
                            fusb302b::TransientTransportFault::RetryFailed,
                            now,
                        )
                        .await;
                }
                Fusb302bReceiveEvent::Protection => {
                    self.clear_vbus_low_interlock();
                    self.policy.mark_fault();
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_PROTECTION, Ordering::Relaxed);
                    return false;
                }
                Fusb302bReceiveEvent::UnsupportedSop => {
                    self.clear_vbus_low_interlock();
                    // A malformed or non-SOP frame is a recoverable receive
                    // condition. Flush the incomplete FIFO state and requery
                    // capabilities without withdrawing Rd or latching the
                    // whole sink policy in Fault.
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_UNSUPPORTED_SOP, Ordering::Relaxed);
                    return self
                        .recover_transient_transport_fault(
                            i2c,
                            fusb302b::TransientTransportFault::ReceiveIoError,
                            now,
                        )
                        .await;
                }
            }
        }

        self.policy.phase() != SinkPhase::Fault
    }
}
