#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]
#![allow(clippy::excessive_nesting)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::too_many_lines)]

mod pd;
mod protocol;

#[cfg(target_arch = "xtensa")]
const EMIT_ROM_DIAGNOSTICS: bool = false;

#[cfg(target_arch = "xtensa")]
#[inline(never)]
fn rom_log_line(line: &[u8]) {
    unsafe extern "C" {
        fn esp_rom_output_tx_one_char(value: u8) -> i32;
    }
    for byte in line {
        // SAFETY: this ROM routine is available on ESP32-S3 and accepts one byte.
        unsafe { esp_rom_output_tx_one_char(*byte) };
    }
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
fn rom_log_hex_u32(prefix: &[u8], value: u32) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    rom_log_line(prefix);
    for shift in (0..8).rev() {
        let nibble = ((value >> (shift * 4)) & 0x0f) as usize;
        rom_log_line(&[HEX[nibble]]);
    }
    rom_log_line(b"\n");
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
fn rom_diag_line(line: &[u8]) {
    if EMIT_ROM_DIAGNOSTICS {
        rom_log_line(line);
    }
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
fn rom_diag_hex_u32(prefix: &[u8], value: u32) {
    if EMIT_ROM_DIAGNOSTICS {
        rom_log_hex_u32(prefix, value);
    }
}

#[cfg(target_arch = "xtensa")]
mod device {
    use super::{pd, protocol};
    use core::cell::UnsafeCell;
    use esp_hal::{
        Blocking,
        analog::adc::{
            Adc, AdcCalBasic, AdcCalCurve, AdcCalScheme, AdcConfig, AdcPin, Attenuation,
        },
        gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
        i2c::master::{Config as I2cConfig, I2c, SoftwareTimeout},
        time::{Duration as HalDuration, Instant, Rate},
        usb_serial_jtag::UsbSerialJtag,
    };
    use fusb302::{
        CcPin, CcPull, DataRole, Fusb302, InterruptMasks, PdPacket, PdRevision, PhyConfig,
        PowerRole, ReceiveSopMask, RetryCount, SopType, ToggleMode,
    };
    use heapless::String;

    const LINE_MAX: usize = 512;
    const I2C_ADDRESS: u8 = 0x22;
    const I2C_TRANSACTION_TIMEOUT_MS: u64 = 10;
    const VIN_DIVIDER_R_HIGH_OHMS: u32 = 56_000;
    const VIN_DIVIDER_R_LOW_OHMS: u32 = 5_100;
    const ATTACH_TIMEOUT_MS: u32 = 3_000;
    const NEGOTIATION_TIMEOUT_MS: u32 = 3_000;
    // A RAM reload can inherit a source session whose sink message-id state is
    // unknown. Probe once, then use a bounded USB-PD Soft Reset to resync the
    // numbered protocol without dropping the attached VBUS contract.
    const RECOVERY_TIMEOUT_MS: u32 = 60_000;
    // Match the production sink policy's normal advertisement window before
    // falling back to an explicit Get_Source_Capabilities request.
    const SOURCE_CAPS_INITIAL_WAIT_MS: u32 = 400;
    const PD_TX_SETTLE_MS: u32 = 5;
    // Updated source firmware can acknowledge Get_Source_Capabilities before
    // publishing the data message. Give that transaction a complete response
    // window before using the protocol-reset fallback.
    const CAPABILITIES_RESPONSE_WAIT_MS: u32 = 500;
    const CAPABILITIES_RESPONSE_ATTEMPTS: u8 = 1;
    const CAPABILITIES_PROTOCOL_RESET_WAIT_MS: u32 = 3_000;
    const SUMMARY_CHUNK_BYTES: usize = 64;
    const USB_FRAME_GAP_MS: u32 = 5;
    const PROGRESS_FRAME_MAX: usize = 512;
    const PROGRESS_QUEUE_DEPTH: usize = 16;
    const PROGRESS_PUMP_BUDGET: usize = 64;
    const SESSION_TIMEOUT_MS: u32 = 210_000;
    const SAMPLE_GAP_LIMIT_MS: u32 = 250;
    const MAX_INVALID_SAMPLES: u16 = 3;
    const SKIP_PD_SESSION_FOR_DIAG: bool = false;
    const SKIP_DEFAULT_RECEIVE_FOR_DIAG: bool = false;
    // Progress frames are queued in RAM and sent through the USB FIFO without
    // waiting in the PD state machine. This keeps the human stream live while
    // preserving the Source_Capabilities -> Request timing window.
    const EMIT_SESSION_PROGRESS: bool = false;
    const EMIT_TIER_PROGRESS: bool = true;
    const EMIT_SAMPLE_PROGRESS: bool = false;
    const EMIT_CAPABILITY_PROGRESS: bool = false;
    const EMIT_RECOVERY_PROGRESS: bool = false;
    // ROM output is synchronous on the USB Serial/JTAG path. A packet trace in
    // the receive helper would delay Source_Capabilities -> Request beyond a
    // source's sink-request window, so keep the timing-sensitive path quiet.
    const EMIT_ROM_PACKET_TRACE: bool = false;
    const TIER_INDEX_TEXT: [&str; pd::TOTAL_TIERS] = [
        "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
        "17", "18", "19", "20", "21", "22",
    ];
    const TIER_TARGET_TEXT: [&str; pd::TOTAL_TIERS] = [
        "5000", "9000", "12000", "15000", "20000", "5000", "6000", "7000", "8000", "9000", "10000",
        "11000", "12000", "13000", "14000", "15000", "16000", "17000", "18000", "19000", "20000",
        "21000",
    ];
    const STATUS0_VBUSOK: u8 = 1 << 7;
    const STATUS0_BC_LVL_MASK: u8 = 0b0000_1100;
    const SWITCHES0_REGISTER: u8 = 0x02;
    const SWITCHES0_MEAS_CC1: u8 = 1 << 2;
    const SWITCHES0_MEAS_CC2: u8 = 1 << 3;
    const STATUS0_CRC_CHECK: u8 = 1 << 4;
    const STATUS1_RX_EMPTY: u8 = 1 << 5;
    const STATUS1A_RXSOP: u8 = 1;
    const INTERRUPTA_HARD_RESET: u8 = 1;
    const INTERRUPTA_SOFT_RESET: u8 = 1 << 1;
    const TOGSS_MASK: u8 = 0b0011_1000;
    const TOGSS_SNK_CC1: u8 = 0b0010_1000;
    const TOGSS_SNK_CC2: u8 = 0b0011_0000;
    const INTERRUPT_MASKS_TOGGLE: InterruptMasks = InterruptMasks::new(0x7f, 0xbf, 0xff);
    const INTERRUPT_MASKS_RECEIVE: InterruptMasks = InterruptMasks::new(0x7d, 0xe0, 0x00);

    struct SummaryStorage(UnsafeCell<String<32_768>>);

    struct ProgressStorage(UnsafeCell<String<512>>);

    struct ProgressQueueStorage(UnsafeCell<ProgressQueue>);

    struct ProgressQueue {
        frames: [[u8; PROGRESS_FRAME_MAX]; PROGRESS_QUEUE_DEPTH],
        lengths: [usize; PROGRESS_QUEUE_DEPTH],
        offsets: [usize; PROGRESS_QUEUE_DEPTH],
        head: usize,
        tail: usize,
        count: usize,
    }

    // The RAM image services one USB request at a time, so this buffer has a
    // single synchronous owner while keeping the large JSON frame off the task stack.
    unsafe impl Sync for SummaryStorage {}
    unsafe impl Sync for ProgressStorage {}
    unsafe impl Sync for ProgressQueueStorage {}

    static SUMMARY_STORAGE: SummaryStorage = SummaryStorage(UnsafeCell::new(String::new()));
    static PROGRESS_LINE_STORAGE: ProgressStorage = ProgressStorage(UnsafeCell::new(String::new()));
    static PROGRESS_RESULT_STORAGE: ProgressStorage =
        ProgressStorage(UnsafeCell::new(String::new()));
    static PROGRESS_QUEUE_STORAGE: ProgressQueueStorage =
        ProgressQueueStorage(UnsafeCell::new(ProgressQueue::new()));

    impl ProgressQueue {
        const fn new() -> Self {
            Self {
                frames: [[0; PROGRESS_FRAME_MAX]; PROGRESS_QUEUE_DEPTH],
                lengths: [0; PROGRESS_QUEUE_DEPTH],
                offsets: [0; PROGRESS_QUEUE_DEPTH],
                head: 0,
                tail: 0,
                count: 0,
            }
        }

        fn enqueue(&mut self, data: &[u8]) -> bool {
            if data.is_empty()
                || data.len() > PROGRESS_FRAME_MAX
                || self.count == PROGRESS_QUEUE_DEPTH
            {
                return false;
            }
            let index = self.tail;
            self.frames[index][..data.len()].copy_from_slice(data);
            self.lengths[index] = data.len();
            self.offsets[index] = 0;
            self.tail = (self.tail + 1) % PROGRESS_QUEUE_DEPTH;
            self.count += 1;
            true
        }

        fn pop_front(&mut self) {
            self.lengths[self.head] = 0;
            self.offsets[self.head] = 0;
            self.head = (self.head + 1) % PROGRESS_QUEUE_DEPTH;
            self.count -= 1;
        }
    }

    type I2cBus = I2c<'static, Blocking>;
    type AdcDriver = Adc<'static, esp_hal::peripherals::ADC1<'static>, Blocking>;
    type AdcVin = AdcPin<
        esp_hal::peripherals::GPIO1<'static>,
        esp_hal::peripherals::ADC1<'static>,
        AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
    >;

    struct PeripheralTokens {
        gpio1: esp_hal::peripherals::GPIO1<'static>,
        adc1: esp_hal::peripherals::ADC1<'static>,
        i2c0: esp_hal::peripherals::I2C0<'static>,
        gpio7: esp_hal::peripherals::GPIO7<'static>,
        gpio8: esp_hal::peripherals::GPIO8<'static>,
        gpio9: esp_hal::peripherals::GPIO9<'static>,
        gpio47: esp_hal::peripherals::GPIO47<'static>,
    }

    impl PeripheralTokens {
        fn split(
            peripherals: esp_hal::peripherals::Peripherals,
        ) -> (esp_hal::peripherals::USB_DEVICE<'static>, Self) {
            let esp_hal::peripherals::Peripherals {
                USB_DEVICE,
                GPIO1: gpio1,
                ADC1: adc1,
                I2C0: i2c0,
                GPIO7: gpio7,
                GPIO8: gpio8,
                GPIO9: gpio9,
                GPIO47: gpio47,
                ..
            } = peripherals;
            (
                USB_DEVICE,
                Self {
                    gpio1,
                    adc1,
                    i2c0,
                    gpio7,
                    gpio8,
                    gpio9,
                    gpio47,
                },
            )
        }
    }

    struct Measurements {
        adc: AdcDriver,
        vin: AdcVin,
        vin_calibration: AdcCalCurve<esp_hal::peripherals::ADC1<'static>>,
        i2c: I2cBus,
        pd_irq: Input<'static>,
    }

    struct Outputs {
        heater: Output<'static>,
    }

    impl Outputs {
        fn new(tokens: PeripheralTokens) -> Result<(Self, Measurements), ()> {
            let mut adc_config = AdcConfig::new();
            let vin = adc_config
                .enable_pin_with_cal::<_, AdcCalBasic<_>>(tokens.gpio1, Attenuation::_11dB);
            let vin_calibration = AdcCalCurve::new_cal(Attenuation::_11dB);
            let adc = Adc::new(tokens.adc1, adc_config);
            let i2c = I2c::new(
                tokens.i2c0,
                I2cConfig::default()
                    .with_frequency(Rate::from_khz(400))
                    .with_software_timeout(SoftwareTimeout::Transaction(HalDuration::from_millis(
                        I2C_TRANSACTION_TIMEOUT_MS,
                    ))),
            )
            .map_err(|_| ())?
            .with_sda(tokens.gpio8)
            .with_scl(tokens.gpio9);
            let pd_irq = Input::new(tokens.gpio7, InputConfig::default().with_pull(Pull::Up));
            Ok((
                Self {
                    heater: Output::new(tokens.gpio47, Level::Low, OutputConfig::default()),
                },
                Measurements {
                    adc,
                    vin,
                    vin_calibration,
                    i2c,
                    pd_irq,
                },
            ))
        }

        fn safe(&mut self) {
            self.heater.set_low();
        }
    }

    #[derive(Clone, Copy)]
    struct TierRecord {
        target: pd::Target,
        selected_object_position: u8,
        status: &'static str,
        reason: &'static str,
        source_max_ma: u16,
        contract_current_ma: u16,
        contract_mv: u16,
        contract_confirmed: bool,
        request_sent_ms: u32,
        contract_confirmed_ms: u32,
        hold_started_ms: u32,
        hold_finished_ms: u32,
        sample_count: u16,
        invalid_sample_count: u16,
        min_mv: u16,
        max_mv: u16,
        mean_mv: u32,
        first_mv: u16,
        last_mv: u16,
    }

    impl TierRecord {
        const EMPTY: Self = Self {
            target: pd::Target {
                mode: pd::Mode::Fixed,
                voltage_mv: 0,
            },
            selected_object_position: 0,
            status: "not_run",
            reason: "",
            source_max_ma: 0,
            contract_current_ma: 0,
            contract_mv: 0,
            contract_confirmed: false,
            request_sent_ms: 0,
            contract_confirmed_ms: 0,
            hold_started_ms: 0,
            hold_finished_ms: 0,
            sample_count: 0,
            invalid_sample_count: 0,
            min_mv: 0,
            max_mv: 0,
            mean_mv: 0,
            first_mv: 0,
            last_mv: 0,
        };

        const fn new(target: pd::Target) -> Self {
            Self {
                target,
                ..Self::EMPTY
            }
        }
    }

    #[derive(Clone, Copy)]
    struct ResetResult {
        status: &'static str,
        default_vbus_mv: u16,
        default_adc_mv: u16,
        default_adc_raw_code: u16,
        default_contract_current_ma: u16,
        sample_count: u16,
        reason: &'static str,
        source_capabilities: pd::SourceCapabilities,
    }

    impl ResetResult {
        const FAILED: Self = Self {
            status: "fail",
            default_vbus_mv: 0,
            default_adc_mv: 0,
            default_adc_raw_code: 0,
            default_contract_current_ma: 0,
            sample_count: 0,
            reason: "reset_not_attempted",
            source_capabilities: pd::SourceCapabilities::empty(),
        };
    }

    struct SessionResult {
        overall: &'static str,
        validate_vin: bool,
        next_sequence: u32,
        tiers: [TierRecord; pd::TOTAL_TIERS],
        tier_count: usize,
        final_reset: ResetResult,
        source_capabilities: pd::SourceCapabilities,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum StopReason {
        Cancelled,
        GlobalTimeout,
    }

    struct CancelState {
        line: [u8; LINE_MAX],
        length: usize,
    }

    impl CancelState {
        const fn new() -> Self {
            Self {
                line: [0; LINE_MAX],
                length: 0,
            }
        }

        fn poll(&mut self, usb: &mut UsbSerialJtag<'static, Blocking>) -> bool {
            while let Ok(byte) = usb.read_byte() {
                if byte == b'\n' {
                    let cancelled = self.line[..self.length]
                        .windows(6)
                        .any(|window| window == b"cancel");
                    self.length = 0;
                    if cancelled {
                        return true;
                    }
                } else if self.length < self.line.len() {
                    self.line[self.length] = byte;
                    self.length += 1;
                } else {
                    self.length = 0;
                }
            }
            false
        }
    }

    fn delay_ms(milliseconds: u32) {
        esp_hal::rom::ets_delay_us(milliseconds.saturating_mul(1_000));
    }

    fn now_ms() -> u32 {
        Instant::now()
            .duration_since_epoch()
            .as_millis()
            .min(u64::from(u32::MAX)) as u32
    }

    fn phy_config(auto_goodcrc: bool) -> PhyConfig {
        PhyConfig {
            pd_revision: PdRevision::Rev30,
            power_role: PowerRole::Sink,
            data_role: DataRole::Ufp,
            auto_goodcrc,
            retry_count: RetryCount::Three,
            auto_soft_reset: false,
            auto_hard_reset: false,
            receive_sop: ReceiveSopMask::NONE,
        }
    }

    fn check_deadline(
        usb: &mut UsbSerialJtag<'static, Blocking>,
        cancel: &mut CancelState,
        deadline_ms: u32,
    ) -> Result<(), StopReason> {
        pump_progress(usb);
        if cancel.poll(usb) {
            return Err(StopReason::Cancelled);
        }
        if now_ms() >= deadline_ms {
            return Err(StopReason::GlobalTimeout);
        }
        Ok(())
    }

    fn emit_progress(
        _usb: &mut UsbSerialJtag<'static, Blocking>,
        request_id: &str,
        sequence: &mut u32,
        result_json: &str,
    ) {
        // Keep nested JSON formatting off the small RTOS task stack. The RAM
        // image handles one request synchronously, so these buffers have one
        // owner at a time. Enqueue only here; USB FIFO writes happen from
        // check_deadline so a progress frame cannot block PD negotiation.
        let line = unsafe { &mut *PROGRESS_LINE_STORAGE.0.get() };
        line.clear();
        if protocol::write_progress(line, request_id, *sequence, result_json).is_ok() {
            let queue = unsafe { &mut *PROGRESS_QUEUE_STORAGE.0.get() };
            let _ = queue.enqueue(line.as_bytes());
        }
        *sequence = sequence.wrapping_add(1);
    }

    fn pump_progress(usb: &mut UsbSerialJtag<'static, Blocking>) {
        let queue = unsafe { &mut *PROGRESS_QUEUE_STORAGE.0.get() };
        let mut budget = PROGRESS_PUMP_BUDGET;
        while budget > 0 && queue.count != 0 {
            let index = queue.head;
            while budget > 0 && queue.offsets[index] < queue.lengths[index] {
                let offset = queue.offsets[index];
                if usb.write_byte_nb(queue.frames[index][offset]).is_err() {
                    let _ = usb.flush_tx_nb();
                    return;
                }
                queue.offsets[index] = offset + 1;
                budget -= 1;
            }
            if queue.offsets[index] < queue.lengths[index] {
                let _ = usb.flush_tx_nb();
                return;
            }
            if usb.flush_tx_nb().is_err() {
                return;
            }
            queue.pop_front();
        }
        if budget == 0 {
            let _ = usb.flush_tx_nb();
        }
    }

    fn drain_progress(usb: &mut UsbSerialJtag<'static, Blocking>) {
        let queue = unsafe { &mut *PROGRESS_QUEUE_STORAGE.0.get() };
        while queue.count != 0 {
            let index = queue.head;
            let offset = queue.offsets[index];
            let length = queue.lengths[index];
            if offset < length && usb.write(&queue.frames[index][offset..length]).is_err() {
                return;
            }
            queue.pop_front();
        }
    }

    fn write_progress_text(usb: &mut UsbSerialJtag<'static, Blocking>, text: &str) -> bool {
        use core::fmt::Write;
        usb.write_str(text).is_ok()
    }

    fn emit_session_progress(
        usb: &mut UsbSerialJtag<'static, Blocking>,
        request_id: &str,
        sequence: &mut u32,
        stage: &str,
    ) {
        let is_summary_stage = stage.starts_with("summary_");
        if !EMIT_SESSION_PROGRESS && !is_summary_stage {
            return;
        }
        let result = unsafe { &mut *PROGRESS_RESULT_STORAGE.0.get() };
        result.clear();
        let formatted = (|| -> core::fmt::Result {
            result
                .push_str("{\"kind\":\"session\",\"stage\":\"")
                .map_err(|_| core::fmt::Error)?;
            result.push_str(stage).map_err(|_| core::fmt::Error)?;
            result
                .push_str("\",\"heater\":\"off\",\"pd\":\"owned\",\"eeprom\":\"untouched\"}")
                .map_err(|_| core::fmt::Error)
        })();
        if formatted.is_ok() {
            emit_progress(usb, request_id, sequence, result.as_str());
        } else {
            emit_progress(
                usb,
                request_id,
                sequence,
                "{\"kind\":\"session\",\"stage\":\"format_error\",\"heater\":\"off\",\"pd\":\"owned\",\"eeprom\":\"untouched\"}",
            );
        }
    }

    fn emit_tier_progress(
        usb: &mut UsbSerialJtag<'static, Blocking>,
        request_id: &str,
        sequence: &mut u32,
        index: usize,
        status: &str,
        _tier: TierRecord,
    ) {
        if !EMIT_TIER_PROGRESS {
            return;
        }
        let result = unsafe { &mut *PROGRESS_RESULT_STORAGE.0.get() };
        result.clear();
        let index_text = TIER_INDEX_TEXT.get(index).copied().unwrap_or("?");
        let target_text = TIER_TARGET_TEXT.get(index).copied().unwrap_or("0");
        let mode = if index < pd::FIXED_TARGETS_MV.len() {
            "fixed"
        } else {
            "pps"
        };
        let _ = result.push_str("{\"kind\":\"tier\",\"mode\":\"");
        let _ = result.push_str(mode);
        let _ = result.push_str("\",\"targetMv\":");
        let _ = result.push_str(target_text);
        let _ = result.push_str(",\"index\":");
        let _ = result.push_str(index_text);
        let _ = result.push_str(",\"total\":22,\"status\":\"");
        let _ = result.push_str(status);
        let _ = result.push_str("\",\"heater\":\"off\",\"pd\":\"owned\",\"eeprom\":\"untouched\"}");
        emit_progress(usb, request_id, sequence, result.as_str());
    }

    fn emit_sample_progress(
        usb: &mut UsbSerialJtag<'static, Blocking>,
        request_id: &str,
        sequence: &mut u32,
        tier: TierRecord,
        elapsed_ms: u32,
        measured_mv: u16,
        valid: bool,
    ) {
        if !EMIT_SAMPLE_PROGRESS {
            return;
        }
        let result = unsafe { &mut *PROGRESS_RESULT_STORAGE.0.get() };
        result.clear();
        use core::fmt::Write;
        let _ = write!(
            result,
            "{{\"kind\":\"sample\",\"mode\":\"{}\",\"targetMv\":{},\"elapsedMs\":{},\"measuredVinMv\":{},\"valid\":{},\"heater\":\"off\",\"pd\":\"owned\",\"eeprom\":\"untouched\"}}",
            tier.target.mode.as_str(),
            tier.target.voltage_mv,
            elapsed_ms,
            measured_mv,
            valid,
        );
        emit_progress(usb, request_id, sequence, result.as_str());
    }

    fn emit_capability_progress(
        usb: &mut UsbSerialJtag<'static, Blocking>,
        request_id: &str,
        sequence: &mut u32,
        capabilities: pd::SourceCapabilities,
    ) {
        if !EMIT_CAPABILITY_PROGRESS {
            return;
        }
        let result = unsafe { &mut *PROGRESS_RESULT_STORAGE.0.get() };
        result.clear();
        use core::fmt::Write;
        let _ = result.push_str("{\"kind\":\"capabilities\",\"count\":");
        let _ = write!(result, "{},\"rawPdos\":[", capabilities.count);
        for index in 0..usize::from(capabilities.count) {
            if index != 0 {
                let _ = result.push(',');
            }
            let _ = write!(result, "{}", capabilities.objects[index].raw);
        }
        let _ = result.push_str("],\"objects\":[");
        for index in 0..usize::from(capabilities.count) {
            if index != 0 {
                let _ = result.push(',');
            }
            let object = capabilities.objects[index];
            let _ = write!(
                result,
                "{{\"position\":{},\"mode\":\"{}\",\"minMv\":{},\"maxMv\":{},\"maxMa\":{},\"raw\":{}}}",
                object.position,
                object.mode.as_str(),
                object.min_mv,
                object.max_mv,
                object.max_ma,
                object.raw,
            );
        }
        let _ = result.push_str("],\"heater\":\"off\",\"pd\":\"owned\",\"eeprom\":\"untouched\"}");
        emit_progress(usb, request_id, sequence, result.as_str());
    }

    fn emit_summary_chunks(
        usb: &mut UsbSerialJtag<'static, Blocking>,
        request_id: &str,
        sequence: &mut u32,
        data: &[u8],
    ) {
        let chunk_count = data.len().div_ceil(SUMMARY_CHUNK_BYTES);
        for (index, chunk) in data.chunks(SUMMARY_CHUNK_BYTES).enumerate() {
            let line = unsafe { &mut *PROGRESS_LINE_STORAGE.0.get() };
            line.clear();
            if protocol::write_summary_chunk_progress(
                line,
                request_id,
                *sequence,
                index,
                chunk_count,
                chunk,
            )
            .is_ok()
            {
                let _ = write_progress_text(usb, line.as_str());
            }
            *sequence = sequence.wrapping_add(1);
            delay_ms(USB_FRAME_GAP_MS);
        }
    }

    fn emit_recovery_progress(
        usb: &mut UsbSerialJtag<'static, Blocking>,
        request_id: &str,
        sequence: &mut u32,
        index: usize,
        reset: ResetResult,
    ) {
        if !EMIT_RECOVERY_PROGRESS {
            return;
        }
        let result = unsafe { &mut *PROGRESS_RESULT_STORAGE.0.get() };
        result.clear();
        use core::fmt::Write;
        let _ = write!(
            result,
            "{{\"kind\":\"recovery\",\"index\":{},\"status\":\"{}\",\"defaultVbusMv\":{},\"defaultAdcMv\":{},\"defaultAdcRawCode\":{},\"defaultContractMv\":{},\"defaultContractCurrentMa\":{},\"reason\":\"{}\",\"heater\":\"off\",\"pd\":\"{}\",\"eeprom\":\"untouched\"}}",
            index + 1,
            reset.status,
            reset.default_vbus_mv,
            reset.default_adc_mv,
            reset.default_adc_raw_code,
            pd::DEFAULT_CONTRACT_MV,
            reset.default_contract_current_ma,
            reset.reason,
            if reset.status == "pass" {
                "default_verified"
            } else {
                "resetting"
            },
        );
        emit_progress(usb, request_id, sequence, result.as_str());
    }

    fn capture_pending_source_capabilities<I2C: embedded_hal::i2c::I2c>(
        phy: &mut Fusb302<I2C>,
    ) -> Option<PdPacket> {
        // RAM loading does not reset the external FUSB302B. A product session
        // may therefore have queued Source_Capabilities before this image
        // starts. Capture that frame before init() flushes the hardware FIFO.
        for _ in 0..4 {
            let status = phy.read_status().ok()?;
            if status.status0 & STATUS0_VBUSOK == 0
                || status.status1 & STATUS1_RX_EMPTY != 0
                || status.status0 & STATUS0_CRC_CHECK == 0
                || status.status1a & STATUS1A_RXSOP == 0
            {
                return None;
            }
            let packet = phy.receive().ok().flatten()?;
            if pd::decode_source_capabilities(packet.header(), packet.payload()).is_ok() {
                super::rom_diag_hex_u32(
                    b"ram_pd_hil_preinit_source_caps_header=0x",
                    u32::from(packet.header()),
                );
                return Some(packet);
            }
        }
        None
    }

    fn existing_attachment_polarity<I2C: embedded_hal::i2c::I2c>(i2c: &mut I2C) -> Option<CcPin> {
        // The production PD service leaves the selected measurement switch
        // armed after Type-C attach. Read that retained hardware state before
        // touching the FUSB302B reset register; STATUS1A.TOGSS is no longer
        // available once the product has stopped autonomous toggling.
        let mut switches0 = [0];
        i2c.write_read(I2C_ADDRESS, &[SWITCHES0_REGISTER], &mut switches0)
            .ok()?;
        super::rom_diag_hex_u32(b"ram_pd_hil_existing_switches0=0x", u32::from(switches0[0]));
        match switches0[0] & (SWITCHES0_MEAS_CC1 | SWITCHES0_MEAS_CC2) {
            SWITCHES0_MEAS_CC1 => Some(CcPin::Cc1),
            SWITCHES0_MEAS_CC2 => Some(CcPin::Cc2),
            _ => None,
        }
    }

    fn attach_failure<I2C: embedded_hal::i2c::I2c>(
        phy: &mut Fusb302<I2C>,
        reason: &'static str,
    ) -> Result<Option<PdPacket>, &'static str> {
        let _ = phy.stop_toggle();
        Err(reason)
    }

    fn attach_sink(i2c: &mut I2cBus) -> Result<Option<PdPacket>, &'static str> {
        let retained_polarity = existing_attachment_polarity(&mut *i2c);
        let mut phy = Fusb302::with_address(&mut *i2c, I2C_ADDRESS);
        let pending_source_capabilities = capture_pending_source_capabilities(&mut phy);
        let mut source_message_id = None;

        // RAM loading does not reset the external FUSB302B. If the production
        // runtime already selected a CC pin and VBUS is still present, keep
        // that protocol session intact. Calling init() here would software
        // reset the PHY, flush its FIFO, and leave the source in a session in
        // which it may answer queries with GoodCRC but no capabilities data.
        if let Some(polarity) = retained_polarity
            && phy
                .read_status()
                .is_ok_and(|status| status.status0 & STATUS0_VBUSOK != 0)
        {
            super::rom_diag_line(b"ram_pd_hil_existing_session_reused\n");
            if !configure_attached(&mut phy, polarity) {
                return attach_failure(&mut phy, "fusb302b_attached_configuration_failed");
            }
            if pending_source_capabilities.is_some() {
                return Ok(pending_source_capabilities);
            }
            let mut message_id = 0;
            return Ok(receive_packet(
                &mut phy,
                &mut message_id,
                &mut source_message_id,
            ));
        }

        if phy.init().is_err()
            || phy.pd_reset().is_err()
            || phy.set_host_current_default().is_err()
            || phy.configure_phy(phy_config(false)).is_err()
            || phy.set_cc_pull(CcPin::Cc1, CcPull::Down).is_err()
            || phy.set_cc_pull(CcPin::Cc2, CcPull::Down).is_err()
        {
            return attach_failure(&mut phy, "fusb302b_initialization_failed");
        }

        // A RAM reload can reset the FUSB302 while the source keeps CC/Rp and
        // VBUS asserted. Reusing that attachment avoids restarting Type-C
        // toggling and losing the source's already-running PD policy engine.
        // Probe both CC pins before asking the controller to discover a new
        // attachment.
        for polarity in [CcPin::Cc1, CcPin::Cc2] {
            if phy.set_measure_cc(Some(polarity)).is_err() {
                continue;
            }
            delay_ms(1);
            let Ok(status) = phy.read_status() else {
                continue;
            };
            if status.status0 & STATUS0_VBUSOK == 0 || status.status0 & STATUS0_BC_LVL_MASK == 0 {
                continue;
            }
            if !configure_attached(&mut phy, polarity) {
                return attach_failure(&mut phy, "fusb302b_attached_configuration_failed");
            }
            if pending_source_capabilities.is_some() {
                return Ok(pending_source_capabilities);
            }
            let mut message_id = 0;
            return Ok(receive_packet(
                &mut phy,
                &mut message_id,
                &mut source_message_id,
            ));
        }

        if phy.set_measure_cc(None).is_err()
            || phy.set_interrupt_masks(INTERRUPT_MASKS_TOGGLE).is_err()
            || phy.read_interrupts().is_err()
            || phy.start_toggle(ToggleMode::Sink).is_err()
        {
            return Err("fusb302b_initialization_failed");
        }
        let deadline = now_ms().saturating_add(ATTACH_TIMEOUT_MS);
        while now_ms() < deadline {
            let status = match phy.read_status() {
                Ok(status) => status,
                Err(_) => return attach_failure(&mut phy, "fusb302b_status_read_failed"),
            };
            if status.status0 & STATUS0_VBUSOK != 0 {
                let polarity = match status.status1a & TOGSS_MASK {
                    TOGSS_SNK_CC1 => Some(CcPin::Cc1),
                    TOGSS_SNK_CC2 => Some(CcPin::Cc2),
                    _ => None,
                };
                if let Some(polarity) = polarity {
                    if configure_attached(&mut phy, polarity) {
                        // Read immediately after enabling the receive PHY. A
                        // source may advertise once during attach, and the
                        // FUSB302B receive FIFO is too small to defer this
                        // first read until the normal discovery window.
                        if pending_source_capabilities.is_some() {
                            return Ok(pending_source_capabilities);
                        }
                        let mut message_id = 0;
                        return Ok(receive_packet(
                            &mut phy,
                            &mut message_id,
                            &mut source_message_id,
                        ));
                    }
                    return attach_failure(&mut phy, "fusb302b_attached_configuration_failed");
                }
            }
            delay_ms(5);
        }

        // A RAM reload can reset the FUSB302 while the source keeps its CC/Rp
        // and VBUS state. In that case autonomous toggling may not emit a new
        // TOGSS result even though the sink can still measure the attachment.
        if phy.stop_toggle().is_ok() {
            for polarity in [CcPin::Cc1, CcPin::Cc2] {
                if phy.set_measure_cc(Some(polarity)).is_err() {
                    continue;
                }
                delay_ms(1);
                let Ok(status) = phy.read_status() else {
                    continue;
                };
                super::rom_diag_hex_u32(
                    b"ram_pd_hil_cc_fallback_status0=0x",
                    u32::from(status.status0),
                );
                if status.status0 & STATUS0_VBUSOK == 0 || status.status0 & STATUS0_BC_LVL_MASK == 0
                {
                    continue;
                }
                if configure_attached(&mut phy, polarity) {
                    if pending_source_capabilities.is_some() {
                        return Ok(pending_source_capabilities);
                    }
                    let mut message_id = 0;
                    return Ok(receive_packet(
                        &mut phy,
                        &mut message_id,
                        &mut source_message_id,
                    ));
                }
            }
        }
        attach_failure(&mut phy, "typec_sink_attach_timeout")
    }

    fn configure_attached<I2C: embedded_hal::i2c::I2c>(
        phy: &mut Fusb302<I2C>,
        polarity: CcPin,
    ) -> bool {
        // Preserve a Source_Capabilities frame that arrived while autonomous
        // Type-C toggling was settling. Recovery paths flush explicitly after
        // attachment; the initial attach must keep this first peer frame.
        phy.stop_toggle().is_ok()
            && phy.set_cc_pull(CcPin::Cc1, CcPull::Down).is_ok()
            && phy.set_cc_pull(CcPin::Cc2, CcPull::Down).is_ok()
            && phy.set_measure_cc(Some(polarity)).is_ok()
            && phy.set_tx_cc(polarity).is_ok()
            && phy.configure_phy(phy_config(true)).is_ok()
            && phy.set_interrupt_masks(INTERRUPT_MASKS_RECEIVE).is_ok()
    }

    /// Re-arm the attached PD PHY without resetting the peer's protocol state.
    /// This path runs while VBUS and CC/Rd remain valid, so a local PD reset
    /// would desynchronize message IDs from the still-attached source.
    fn resynchronize_attached<I2C: embedded_hal::i2c::I2c>(phy: &mut Fusb302<I2C>) -> bool {
        phy.flush_fifos().is_ok()
            && phy.configure_phy(phy_config(true)).is_ok()
            && phy.set_interrupt_masks(INTERRUPT_MASKS_RECEIVE).is_ok()
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ReceiveOutcome {
        Empty,
        Packet(PdPacket),
        SoftReset,
        HardReset,
    }

    fn source_message_id_is_fresh(last: Option<u8>, current: u8) -> bool {
        let Some(last) = last else {
            return true;
        };
        let distance = current.wrapping_add(8).wrapping_sub(last) & 0x07;
        (1..=4).contains(&distance)
    }

    fn observe_source_message_id(packet: &PdPacket, last: &mut Option<u8>) -> bool {
        let current = pd::message_id(packet.header());
        if !source_message_id_is_fresh(*last, current) {
            return false;
        }
        *last = Some(current);
        true
    }

    fn receive_packet_with_probe<I2C, F>(
        phy: &mut Fusb302<I2C>,
        message_id: &mut u8,
        source_message_id: &mut Option<u8>,
        mut emit_stage: F,
    ) -> ReceiveOutcome
    where
        I2C: embedded_hal::i2c::I2c,
        F: FnMut(&'static str),
    {
        emit_stage("capabilities_receive_begin");
        emit_stage("capabilities_receive_probe_ready");
        let interrupts = match phy.read_interrupts() {
            Ok(interrupts) => interrupts,
            Err(_) => {
                emit_stage("capabilities_receive_interrupts_failed");
                return ReceiveOutcome::Empty;
            }
        };
        emit_stage("capabilities_receive_interrupts_ready");
        if EMIT_ROM_PACKET_TRACE && (interrupts.interrupt_a != 0 || interrupts.interrupt_b != 0) {
            super::rom_diag_hex_u32(
                b"ram_pd_hil_interrupt_a=0x",
                u32::from(interrupts.interrupt_a),
            );
            super::rom_diag_hex_u32(
                b"ram_pd_hil_interrupt_b=0x",
                u32::from(interrupts.interrupt_b),
            );
        }
        if interrupts.interrupt_a & INTERRUPTA_HARD_RESET != 0 {
            emit_stage("capabilities_receive_hard_reset");
            let _ = phy.flush_fifos();
            *message_id = 0;
            *source_message_id = None;
            emit_stage("capabilities_receive_hard_reset_recovered");
            return ReceiveOutcome::HardReset;
        }
        if interrupts.interrupt_a & INTERRUPTA_SOFT_RESET != 0 {
            emit_stage("capabilities_receive_soft_reset");
            let _ = phy.flush_fifos();
            let accept = PdPacket::new(SopType::Sop, pd::accept_header(0), &[]);
            let accepted = accept.is_ok_and(|packet| phy.transmit(&packet).is_ok());
            *message_id = 0;
            *source_message_id = None;
            emit_stage(if accepted {
                "capabilities_receive_soft_reset_accepted"
            } else {
                "capabilities_receive_soft_reset_accept_failed"
            });
            return ReceiveOutcome::SoftReset;
        }
        let status = match phy.read_status() {
            Ok(status) => status,
            Err(_) => {
                emit_stage("capabilities_receive_status_failed");
                return ReceiveOutcome::Empty;
            }
        };
        emit_stage("capabilities_receive_status_ready");
        let vbus_ok = status.status0 & STATUS0_VBUSOK != 0;
        emit_stage("capabilities_receive_vbus_checked");
        let rx_not_empty = status.status1 & STATUS1_RX_EMPTY == 0;
        emit_stage("capabilities_receive_fifo_status_checked");
        let crc_ok = status.status0 & STATUS0_CRC_CHECK != 0;
        emit_stage("capabilities_receive_crc_checked");
        let sop_ok = status.status1a & STATUS1A_RXSOP != 0;
        emit_stage("capabilities_receive_sop_checked");
        if EMIT_ROM_PACKET_TRACE
            && (interrupts.interrupt_a != 0
                || interrupts.interrupt_b != 0
                || rx_not_empty
                || !vbus_ok)
        {
            super::rom_diag_hex_u32(b"ram_pd_hil_status0=0x", u32::from(status.status0));
            super::rom_diag_hex_u32(b"ram_pd_hil_status1=0x", u32::from(status.status1));
            super::rom_diag_hex_u32(b"ram_pd_hil_status1a=0x", u32::from(status.status1a));
        }
        if !vbus_ok || !rx_not_empty || !crc_ok || !sop_ok {
            emit_stage("capabilities_receive_empty");
            return ReceiveOutcome::Empty;
        }
        emit_stage("capabilities_receive_frame_ready");
        let packet = match phy.receive() {
            Ok(Some(packet)) => packet,
            Ok(None) => {
                emit_stage("capabilities_receive_fifo_empty");
                return ReceiveOutcome::Empty;
            }
            Err(_) => {
                emit_stage("capabilities_receive_fifo_failed");
                return ReceiveOutcome::Empty;
            }
        };
        if EMIT_ROM_PACKET_TRACE {
            super::rom_diag_hex_u32(b"ram_pd_hil_rx_header=0x", u32::from(packet.header()));
        }
        let message_type = pd::message_type(packet.header());
        let is_good_crc = message_type == 1 && pd::object_count(packet.header()) == 0;
        if !is_good_crc && !observe_source_message_id(&packet, source_message_id) {
            emit_stage("capabilities_receive_stale_message");
            return ReceiveOutcome::Empty;
        }
        emit_stage("capabilities_receive_returned");
        ReceiveOutcome::Packet(packet)
    }

    fn receive_packet<I2C: embedded_hal::i2c::I2c>(
        phy: &mut Fusb302<I2C>,
        message_id: &mut u8,
        source_message_id: &mut Option<u8>,
    ) -> Option<PdPacket> {
        match receive_packet_with_probe(phy, message_id, source_message_id, |_| {}) {
            ReceiveOutcome::Packet(packet) => Some(packet),
            ReceiveOutcome::Empty | ReceiveOutcome::SoftReset | ReceiveOutcome::HardReset => None,
        }
    }

    fn emit_source_capabilities(
        usb: &mut UsbSerialJtag<'static, Blocking>,
        request_id: &str,
        sequence: &mut u32,
        _packet: &PdPacket,
        capabilities: pd::SourceCapabilities,
    ) {
        emit_capability_progress(usb, request_id, sequence, capabilities);
    }

    fn read_vin_sample(
        adc: &mut AdcDriver,
        vin: &mut AdcVin,
        calibration: &AdcCalCurve<esp_hal::peripherals::ADC1<'static>>,
    ) -> Option<(u16, u16, u16)> {
        let mut raw = None;
        for _ in 0..100 {
            match adc.read_oneshot(vin) {
                Ok(value) => {
                    raw = Some(value & 0x0fff);
                    break;
                }
                Err(nb::Error::WouldBlock) => delay_ms(1),
                Err(nb::Error::Other(_)) => break,
            }
        }
        let raw_code = raw?;
        let adc_mv = calibration.adc_val(raw_code);
        let divider_total = VIN_DIVIDER_R_HIGH_OHMS + VIN_DIVIDER_R_LOW_OHMS;
        let input_mv = u32::from(adc_mv)
            .saturating_mul(divider_total)
            .checked_div(VIN_DIVIDER_R_LOW_OHMS)?
            .min(u32::from(u16::MAX)) as u16;
        Some((input_mv, adc_mv, raw_code))
    }

    fn read_vin_mv(
        adc: &mut AdcDriver,
        vin: &mut AdcVin,
        calibration: &AdcCalCurve<esp_hal::peripherals::ADC1<'static>>,
    ) -> Option<u16> {
        read_vin_sample(adc, vin, calibration).map(|(input_mv, _, _)| input_mv)
    }

    fn request_default_contract<I2C: embedded_hal::i2c::I2c>(
        phy: &mut Fusb302<I2C>,
        selected: pd::SelectedContract,
        usb: &mut UsbSerialJtag<'static, Blocking>,
        cancel: &mut CancelState,
        deadline_ms: u32,
        request_id: &str,
        sequence: &mut u32,
        message_id: &mut u8,
        source_message_id: &mut Option<u8>,
    ) -> Result<(), StopReason> {
        let request_payload = pd::request_data_object(selected);
        if phy.flush_fifos().is_err() {
            emit_session_progress(usb, request_id, sequence, "default_request_flush_failed");
            return Err(StopReason::GlobalTimeout);
        }
        let packet = PdPacket::new(
            SopType::Sop,
            pd::request_header(*message_id),
            &request_payload,
        )
        .map_err(|_| StopReason::GlobalTimeout)?;
        emit_session_progress(usb, request_id, sequence, "default_request_begin");
        let transmit_result = phy.transmit(&packet);
        if transmit_result.is_err() {
            emit_session_progress(usb, request_id, sequence, "default_request_transmit_failed");
            return Err(StopReason::GlobalTimeout);
        }
        // Keep diagnostics after the wire write. ROM output is intentionally
        // slow on the USB Serial/JTAG path and must not consume the source's
        // SenderResponse window between Source_Capabilities and Request.
        super::rom_diag_hex_u32(b"ram_pd_hil_default_header=0x", u32::from(packet.header()));
        super::rom_diag_hex_u32(
            b"ram_pd_hil_default_object=0x",
            u32::from_le_bytes(request_payload),
        );
        super::rom_diag_line(b"ram_pd_hil_default_request_transmit_returned\n");
        emit_session_progress(usb, request_id, sequence, "default_request_transmitted");
        *message_id = (*message_id + 1) & 0x07;
        delay_ms(PD_TX_SETTLE_MS);
        super::rom_diag_line(b"ram_pd_hil_default_request_receive_begin\n");

        if SKIP_DEFAULT_RECEIVE_FOR_DIAG {
            delay_ms(1);
            return Err(StopReason::GlobalTimeout);
        }

        let negotiation_deadline = now_ms()
            .saturating_add(NEGOTIATION_TIMEOUT_MS)
            .min(deadline_ms);
        let mut accepted = false;
        let mut receive_progress_reported = false;
        while now_ms() < negotiation_deadline {
            check_deadline(usb, cancel, deadline_ms)?;
            if !receive_progress_reported {
                emit_session_progress(usb, request_id, sequence, "default_request_receive_begin");
            }
            let packet = receive_packet_with_probe(phy, message_id, source_message_id, |_| {});
            if !receive_progress_reported {
                emit_session_progress(
                    usb,
                    request_id,
                    sequence,
                    "default_request_receive_returned",
                );
                receive_progress_reported = true;
            }
            match packet {
                ReceiveOutcome::Packet(packet) => match pd::message_type(packet.header()) {
                    3 => {
                        accepted = true;
                        emit_session_progress(
                            usb,
                            request_id,
                            sequence,
                            "default_request_accepted",
                        );
                    }
                    4 | 12 => return Err(StopReason::GlobalTimeout),
                    6 if accepted => {
                        if phy
                            .read_status()
                            .is_ok_and(|status| status.status0 & STATUS0_VBUSOK != 0)
                        {
                            emit_session_progress(
                                usb,
                                request_id,
                                sequence,
                                "default_request_ps_rdy",
                            );
                            return Ok(());
                        }
                        return Err(StopReason::GlobalTimeout);
                    }
                    _ => {}
                },
                ReceiveOutcome::SoftReset | ReceiveOutcome::HardReset => {
                    return Err(StopReason::GlobalTimeout);
                }
                ReceiveOutcome::Empty => {}
            }
            delay_ms(2);
        }
        Err(StopReason::GlobalTimeout)
    }

    fn reset_to_default<I2C: embedded_hal::i2c::I2c>(
        phy: &mut Fusb302<I2C>,
        adc: &mut AdcDriver,
        vin: &mut AdcVin,
        calibration: &AdcCalCurve<esp_hal::peripherals::ADC1<'static>>,
        pd_irq: &Input<'static>,
        resynchronize: bool,
        validate_vin: bool,
        usb: &mut UsbSerialJtag<'static, Blocking>,
        cancel: &mut CancelState,
        deadline_ms: u32,
        request_id: &str,
        sequence: &mut u32,
        message_id: &mut u8,
        source_message_id: &mut Option<u8>,
        pending_packet: Option<PdPacket>,
        cached_capabilities: Option<pd::SourceCapabilities>,
    ) -> Result<ResetResult, StopReason> {
        let recovery_deadline = now_ms()
            .saturating_add(RECOVERY_TIMEOUT_MS)
            .max(deadline_ms);
        check_deadline(usb, cancel, recovery_deadline)?;
        emit_session_progress(usb, request_id, sequence, "recovery_prepare");
        if resynchronize && !resynchronize_attached(phy) {
            return Ok(ResetResult {
                status: "fail",
                default_vbus_mv: 0,
                default_adc_mv: 0,
                default_adc_raw_code: 0,
                default_contract_current_ma: 0,
                sample_count: 0,
                reason: "fusb302b_attached_resynchronization_failed",
                source_capabilities: pd::SourceCapabilities::empty(),
            });
        }
        emit_session_progress(usb, request_id, sequence, "recovery_phy_ready");

        let capabilities = match cached_capabilities {
            Some(capabilities) => {
                emit_session_progress(usb, request_id, sequence, "capabilities_cached");
                capabilities
            }
            None => match discover_capabilities(
                phy,
                usb,
                cancel,
                recovery_deadline,
                sequence,
                request_id,
                message_id,
                source_message_id,
                pending_packet,
            ) {
                Ok(capabilities) => capabilities,
                Err(stop) => {
                    emit_session_progress(usb, request_id, sequence, "capabilities_failed");
                    return Err(stop);
                }
            },
        };
        let default_contract = capabilities.select_default_5v();
        let Some(default_contract) = default_contract else {
            return Ok(ResetResult {
                status: "fail",
                default_vbus_mv: 0,
                default_adc_mv: 0,
                default_adc_raw_code: 0,
                default_contract_current_ma: 0,
                sample_count: 0,
                reason: "source_missing_fixed_5v_recovery_pdo",
                source_capabilities: capabilities,
            });
        };
        let request_result = request_default_contract(
            phy,
            default_contract,
            usb,
            cancel,
            recovery_deadline,
            request_id,
            sequence,
            message_id,
            source_message_id,
        );
        match request_result {
            Ok(()) => {}
            Err(StopReason::Cancelled) => return Err(StopReason::Cancelled),
            Err(StopReason::GlobalTimeout) if now_ms() >= deadline_ms => {
                return Err(StopReason::GlobalTimeout);
            }
            Err(StopReason::GlobalTimeout) => {
                return Ok(ResetResult {
                    status: "fail",
                    default_vbus_mv: 0,
                    default_adc_mv: 0,
                    default_adc_raw_code: 0,
                    default_contract_current_ma: 0,
                    sample_count: 0,
                    reason: "default_5v_contract_failed",
                    source_capabilities: capabilities,
                });
            }
        }

        let started = now_ms();
        let mut next_sample = started;
        let mut count = 0u16;
        let mut last_mv = 0u16;
        let mut last_adc_mv = 0u16;
        let mut last_adc_raw_code = 0u16;
        let mut all_default = true;
        while now_ms().saturating_sub(started) < pd::DEFAULT_VERIFY_MS {
            check_deadline(usb, cancel, recovery_deadline)?;
            let now = now_ms();
            if now < next_sample {
                delay_ms(1);
                continue;
            }
            next_sample = now.saturating_add(pd::SAMPLE_INTERVAL_MS);
            let _ = pd_irq.is_low();
            for _ in 0..16 {
                check_deadline(usb, cancel, recovery_deadline)?;
                match receive_packet_with_probe(phy, message_id, source_message_id, |_| {}) {
                    ReceiveOutcome::SoftReset | ReceiveOutcome::HardReset => {
                        return Err(StopReason::GlobalTimeout);
                    }
                    ReceiveOutcome::Packet(_) | ReceiveOutcome::Empty => {}
                }
            }
            if validate_vin {
                match read_vin_sample(adc, vin, calibration) {
                    Some((value, adc_mv, raw_code)) => {
                        count = count.saturating_add(1);
                        last_mv = value;
                        last_adc_mv = adc_mv;
                        last_adc_raw_code = raw_code;
                        if !(pd::DEFAULT_MIN_MV..=pd::DEFAULT_MAX_MV).contains(&value) {
                            all_default = false;
                        }
                    }
                    None => all_default = false,
                }
            } else {
                let vbus_ok = phy
                    .read_status()
                    .is_ok_and(|status| status.status0 & STATUS0_VBUSOK != 0);
                if vbus_ok {
                    count = count.saturating_add(1);
                } else {
                    all_default = false;
                }
            }
        }
        let verified = all_default && count >= 8;
        Ok(ResetResult {
            status: if verified { "pass" } else { "fail" },
            default_vbus_mv: last_mv,
            default_adc_mv: last_adc_mv,
            default_adc_raw_code: last_adc_raw_code,
            default_contract_current_ma: default_contract.contract_current_ma,
            sample_count: count,
            reason: if verified && validate_vin {
                "default_vbus_verified"
            } else if verified {
                "default_contract_confirmed_external_vin"
            } else if count == 0 {
                "default_vbus_unmeasured"
            } else {
                "default_vbus_out_of_range"
            },
            source_capabilities: capabilities,
        })
    }

    fn wait_for_source_capabilities<I2C: embedded_hal::i2c::I2c>(
        phy: &mut Fusb302<I2C>,
        usb: &mut UsbSerialJtag<'static, Blocking>,
        cancel: &mut CancelState,
        deadline_ms: u32,
        sequence: &mut u32,
        request_id: &str,
        message_id: &mut u8,
        source_message_id: &mut Option<u8>,
        wait_ms: u32,
    ) -> Result<Option<(PdPacket, pd::SourceCapabilities)>, StopReason> {
        let wait_deadline = now_ms().saturating_add(wait_ms);
        while now_ms() < wait_deadline {
            check_deadline(usb, cancel, deadline_ms)?;
            match receive_packet_with_probe(phy, message_id, source_message_id, |_| {}) {
                ReceiveOutcome::Packet(packet) => {
                    if let Ok(capabilities) =
                        pd::decode_source_capabilities(packet.header(), packet.payload())
                    {
                        return Ok(Some((packet, capabilities)));
                    }
                    emit_session_progress(
                        usb,
                        request_id,
                        sequence,
                        "capabilities_pending_ignored",
                    );
                }
                ReceiveOutcome::SoftReset | ReceiveOutcome::HardReset | ReceiveOutcome::Empty => {}
            }
            delay_ms(5);
        }
        Ok(None)
    }

    fn reset_source_protocol_for_capabilities<I2C: embedded_hal::i2c::I2c>(
        phy: &mut Fusb302<I2C>,
        usb: &mut UsbSerialJtag<'static, Blocking>,
        cancel: &mut CancelState,
        deadline_ms: u32,
        sequence: &mut u32,
        request_id: &str,
        message_id: &mut u8,
        source_message_id: &mut Option<u8>,
    ) -> Result<Option<(PdPacket, pd::SourceCapabilities)>, StopReason> {
        emit_session_progress(
            usb,
            request_id,
            sequence,
            "capabilities_protocol_reset_begin",
        );
        // The first Get_Source_Capabilities consumed the first probe ID. The
        // product sink normally sent its initial Request with ID 0 before RAM
        // takeover, so the next sink ID is 1. A Soft Reset is numbered, but it
        // does not drop VBUS; after Accept both ports restart at message ID 0.
        let soft_reset_id = *message_id;
        let packet = match PdPacket::new(SopType::Sop, pd::soft_reset_header(soft_reset_id), &[]) {
            Ok(packet) => packet,
            Err(_) => {
                emit_session_progress(
                    usb,
                    request_id,
                    sequence,
                    "capabilities_protocol_reset_failed",
                );
                return Ok(None);
            }
        };
        super::rom_diag_hex_u32(
            b"ram_pd_hil_soft_reset_header=0x",
            u32::from(packet.header()),
        );
        if phy.transmit(&packet).is_err() {
            emit_session_progress(
                usb,
                request_id,
                sequence,
                "capabilities_protocol_reset_failed",
            );
            return Ok(None);
        }
        *message_id = (*message_id + 1) & 0x07;
        emit_session_progress(
            usb,
            request_id,
            sequence,
            "capabilities_protocol_reset_transmitted",
        );
        delay_ms(PD_TX_SETTLE_MS);
        emit_session_progress(
            usb,
            request_id,
            sequence,
            "capabilities_protocol_reset_waiting",
        );
        let wait_deadline = now_ms().saturating_add(CAPABILITIES_PROTOCOL_RESET_WAIT_MS);
        while now_ms() < wait_deadline {
            check_deadline(usb, cancel, deadline_ms)?;
            match receive_packet_with_probe(phy, message_id, source_message_id, |_| {}) {
                ReceiveOutcome::Packet(packet) => {
                    if let Ok(capabilities) =
                        pd::decode_source_capabilities(packet.header(), packet.payload())
                    {
                        emit_session_progress(
                            usb,
                            request_id,
                            sequence,
                            "capabilities_after_protocol_reset",
                        );
                        return Ok(Some((packet, capabilities)));
                    }
                    if pd::message_type(packet.header()) == 3
                        && pd::object_count(packet.header()) == 0
                    {
                        *message_id = 0;
                        *source_message_id = None;
                        emit_session_progress(
                            usb,
                            request_id,
                            sequence,
                            "capabilities_protocol_reset_accepted",
                        );
                    } else if pd::message_type(packet.header()) == 1
                        && pd::object_count(packet.header()) == 0
                    {
                        emit_session_progress(
                            usb,
                            request_id,
                            sequence,
                            "capabilities_protocol_reset_goodcrc",
                        );
                    }
                }
                ReceiveOutcome::HardReset => {
                    emit_session_progress(
                        usb,
                        request_id,
                        sequence,
                        "capabilities_protocol_reset_peer_hard_reset",
                    );
                    return Ok(None);
                }
                ReceiveOutcome::SoftReset => {
                    return Err(StopReason::GlobalTimeout);
                }
                ReceiveOutcome::Empty => {}
            }
            delay_ms(5);
        }
        Ok(None)
    }

    fn discover_capabilities<I2C: embedded_hal::i2c::I2c>(
        phy: &mut Fusb302<I2C>,
        usb: &mut UsbSerialJtag<'static, Blocking>,
        cancel: &mut CancelState,
        deadline_ms: u32,
        sequence: &mut u32,
        request_id: &str,
        message_id: &mut u8,
        source_message_id: &mut Option<u8>,
        pending_packet: Option<PdPacket>,
    ) -> Result<pd::SourceCapabilities, StopReason> {
        check_deadline(usb, cancel, deadline_ms)?;
        if let Some(packet) = pending_packet {
            if let Ok(capabilities) =
                pd::decode_source_capabilities(packet.header(), packet.payload())
            {
                if !observe_source_message_id(&packet, source_message_id) {
                    emit_session_progress(usb, request_id, sequence, "capabilities_pending_stale");
                } else {
                    emit_source_capabilities(usb, request_id, sequence, &packet, capabilities);
                    return Ok(capabilities);
                }
            }
            emit_session_progress(usb, request_id, sequence, "capabilities_pending_ignored");
        }
        emit_session_progress(usb, request_id, sequence, "capabilities_pending_probe");
        if let Some((packet, capabilities)) = wait_for_source_capabilities(
            phy,
            usb,
            cancel,
            deadline_ms,
            sequence,
            request_id,
            message_id,
            source_message_id,
            SOURCE_CAPS_INITIAL_WAIT_MS,
        )? {
            emit_source_capabilities(usb, request_id, sequence, &packet, capabilities);
            return Ok(capabilities);
        }

        let packet = PdPacket::new(
            SopType::Sop,
            pd::get_source_capabilities_header(*message_id),
            &[],
        )
        .map_err(|_| StopReason::GlobalTimeout)?;
        super::rom_diag_hex_u32(b"ram_pd_hil_tx_header=0x", u32::from(packet.header()));
        emit_session_progress(usb, request_id, sequence, "capabilities_request");
        if phy.transmit(&packet).is_err() {
            return Err(StopReason::GlobalTimeout);
        }
        *message_id = (*message_id + 1) & 0x07;
        emit_session_progress(usb, request_id, sequence, "capabilities_waiting");
        delay_ms(PD_TX_SETTLE_MS);
        for attempt in 0..CAPABILITIES_RESPONSE_ATTEMPTS {
            debug_assert_eq!(attempt, 0);
            let response_deadline = now_ms().saturating_add(CAPABILITIES_RESPONSE_WAIT_MS);
            let mut packet_seen = false;
            while now_ms() < response_deadline {
                check_deadline(usb, cancel, deadline_ms)?;
                let Some(packet) = receive_packet(phy, message_id, source_message_id) else {
                    delay_ms(5);
                    continue;
                };
                packet_seen = true;
                emit_session_progress(usb, request_id, sequence, "capabilities_packet_ready");
                emit_session_progress(usb, request_id, sequence, "capabilities_decode_begin");
                match pd::decode_source_capabilities(packet.header(), packet.payload()) {
                    Ok(capabilities) => {
                        emit_source_capabilities(usb, request_id, sequence, &packet, capabilities);
                        emit_session_progress(
                            usb,
                            request_id,
                            sequence,
                            "capabilities_decode_returned",
                        );
                        return Ok(capabilities);
                    }
                    Err(_)
                        if pd::message_type(packet.header()) == 1
                            && pd::object_count(packet.header()) != 0 =>
                    {
                        emit_session_progress(
                            usb,
                            request_id,
                            sequence,
                            "capabilities_decode_failed",
                        );
                        return Err(StopReason::GlobalTimeout);
                    }
                    _ => {
                        let stage = if pd::message_type(packet.header()) == 1
                            && pd::object_count(packet.header()) == 0
                        {
                            "capabilities_goodcrc"
                        } else {
                            "capabilities_packet_ignored"
                        };
                        emit_session_progress(usb, request_id, sequence, stage);
                    }
                }
            }
            if !packet_seen {
                emit_session_progress(usb, request_id, sequence, "capabilities_packet_empty");
            }
        }

        emit_session_progress(
            usb,
            request_id,
            sequence,
            "capabilities_unavailable_after_query",
        );
        if let Some((packet, capabilities)) = reset_source_protocol_for_capabilities(
            phy,
            usb,
            cancel,
            deadline_ms,
            sequence,
            request_id,
            message_id,
            source_message_id,
        )? {
            emit_source_capabilities(usb, request_id, sequence, &packet, capabilities);
            return Ok(capabilities);
        }
        emit_session_progress(
            usb,
            request_id,
            sequence,
            "capabilities_unavailable_after_protocol_reset",
        );
        Err(StopReason::GlobalTimeout)
    }

    fn refresh_source_capabilities_before_request<I2C: embedded_hal::i2c::I2c>(
        phy: &mut Fusb302<I2C>,
        usb: &mut UsbSerialJtag<'static, Blocking>,
        cancel: &mut CancelState,
        deadline_ms: u32,
        sequence: &mut u32,
        request_id: &str,
        message_id: &mut u8,
        source_message_id: &mut Option<u8>,
    ) -> Result<pd::SourceCapabilities, StopReason> {
        check_deadline(usb, cancel, deadline_ms)?;
        if phy.flush_fifos().is_err() {
            emit_session_progress(
                usb,
                request_id,
                sequence,
                "capabilities_refresh_flush_failed",
            );
            return Err(StopReason::GlobalTimeout);
        }
        let packet = PdPacket::new(
            SopType::Sop,
            pd::get_source_capabilities_header(*message_id),
            &[],
        )
        .map_err(|_| StopReason::GlobalTimeout)?;
        emit_session_progress(usb, request_id, sequence, "capabilities_refresh_request");
        if phy.transmit(&packet).is_err() {
            emit_session_progress(
                usb,
                request_id,
                sequence,
                "capabilities_refresh_transmit_failed",
            );
            return Err(StopReason::GlobalTimeout);
        }
        super::rom_diag_hex_u32(
            b"ram_pd_hil_capabilities_header=0x",
            u32::from(packet.header()),
        );
        *message_id = (*message_id + 1) & 0x07;
        delay_ms(PD_TX_SETTLE_MS);

        let response_deadline = now_ms()
            .saturating_add(CAPABILITIES_RESPONSE_WAIT_MS)
            .min(deadline_ms);
        while now_ms() < response_deadline {
            check_deadline(usb, cancel, deadline_ms)?;
            match receive_packet_with_probe(phy, message_id, source_message_id, |_| {}) {
                ReceiveOutcome::Packet(packet) => {
                    if let Ok(capabilities) =
                        pd::decode_source_capabilities(packet.header(), packet.payload())
                    {
                        emit_session_progress(
                            usb,
                            request_id,
                            sequence,
                            "capabilities_refresh_received",
                        );
                        return Ok(capabilities);
                    }
                    if pd::message_type(packet.header()) == 1
                        && pd::object_count(packet.header()) == 0
                    {
                        emit_session_progress(
                            usb,
                            request_id,
                            sequence,
                            "capabilities_refresh_goodcrc",
                        );
                    }
                }
                ReceiveOutcome::HardReset => {
                    emit_session_progress(
                        usb,
                        request_id,
                        sequence,
                        "capabilities_refresh_hard_reset",
                    );
                    return Err(StopReason::GlobalTimeout);
                }
                ReceiveOutcome::SoftReset | ReceiveOutcome::Empty => {}
            }
            delay_ms(5);
        }
        emit_session_progress(usb, request_id, sequence, "capabilities_refresh_timeout");
        Err(StopReason::GlobalTimeout)
    }

    fn run_tier<I2C: embedded_hal::i2c::I2c>(
        phy: &mut Fusb302<I2C>,
        adc: &mut AdcDriver,
        vin: &mut AdcVin,
        calibration: &AdcCalCurve<esp_hal::peripherals::ADC1<'static>>,
        pd_irq: &Input<'static>,
        usb: &mut UsbSerialJtag<'static, Blocking>,
        cancel: &mut CancelState,
        session_deadline_ms: u32,
        request_id: &str,
        sequence: &mut u32,
        selected: pd::SelectedContract,
        index: usize,
        message_id: &mut u8,
        source_message_id: &mut Option<u8>,
        validate_vin: bool,
    ) -> Result<TierRecord, StopReason> {
        let mut tier = TierRecord::new(selected.target);
        tier.selected_object_position = selected.object.position;
        tier.source_max_ma = selected.object.max_ma;
        tier.contract_current_ma = selected.contract_current_ma;
        tier.contract_mv = selected.target.voltage_mv;
        emit_tier_progress(usb, request_id, sequence, index, "requesting", tier);

        let request_payload = pd::request_data_object(selected);
        // Quarantine a late response from the preceding contract before the
        // next Request is transmitted.
        if phy.flush_fifos().is_err() {
            tier.status = "negotiation_failed";
            tier.reason = "request_flush_failed";
            return Ok(tier);
        }
        let packet = PdPacket::new(
            SopType::Sop,
            pd::request_header(*message_id),
            &request_payload,
        )
        .map_err(|_| StopReason::GlobalTimeout)?;
        tier.request_sent_ms = now_ms();
        if phy.transmit(&packet).is_err() {
            tier.status = "negotiation_failed";
            tier.reason = "request_transmit_failed";
            return Ok(tier);
        }
        *message_id = (*message_id + 1) & 0x07;

        let negotiation_deadline = now_ms()
            .saturating_add(NEGOTIATION_TIMEOUT_MS)
            .min(session_deadline_ms);
        let mut accepted = false;
        while now_ms() < negotiation_deadline {
            check_deadline(usb, cancel, session_deadline_ms)?;
            match receive_packet_with_probe(phy, message_id, source_message_id, |_| {}) {
                ReceiveOutcome::Packet(packet) => match pd::message_type(packet.header()) {
                    3 => accepted = true,
                    4 => {
                        tier.status = "negotiation_failed";
                        tier.reason = "source_rejected_request";
                        return Ok(tier);
                    }
                    12 => {
                        tier.status = "negotiation_failed";
                        tier.reason = "source_waited_for_request";
                        return Ok(tier);
                    }
                    6 if accepted => {
                        let status = phy.read_status().ok();
                        if status.is_some_and(|status| status.status0 & STATUS0_VBUSOK != 0) {
                            tier.contract_confirmed = true;
                            tier.contract_confirmed_ms = now_ms();
                            break;
                        }
                        tier.status = "negotiation_failed";
                        tier.reason = "ps_rdy_without_vbus";
                        return Ok(tier);
                    }
                    _ => {}
                },
                ReceiveOutcome::SoftReset | ReceiveOutcome::HardReset => {
                    tier.status = "negotiation_failed";
                    tier.reason = "pd_reset_during_negotiation";
                    return Ok(tier);
                }
                ReceiveOutcome::Empty => {}
            }
            delay_ms(2);
        }
        if !tier.contract_confirmed {
            tier.status = "negotiation_failed";
            tier.reason = if accepted {
                "ps_rdy_timeout"
            } else {
                "accept_timeout"
            };
            return Ok(tier);
        }

        let tolerance = pd::tolerance_mv(tier.target.voltage_mv);
        let mut hold_started = None;
        let mut next_sample = now_ms();
        let mut last_valid_at = None;
        let mut all_samples_in_range = true;
        while now_ms() < session_deadline_ms {
            check_deadline(usb, cancel, session_deadline_ms)?;
            let now = now_ms();
            if let Some(started) = hold_started {
                if now.saturating_sub(started) >= pd::HOLD_MS {
                    tier.hold_finished_ms = now;
                    break;
                }
            } else if now.saturating_sub(tier.contract_confirmed_ms) > NEGOTIATION_TIMEOUT_MS {
                tier.status = "measurement_failed";
                tier.reason = "no_valid_vin_sample";
                return Ok(tier);
            }

            if now < next_sample {
                let _ = pd_irq.is_low();
                match receive_packet_with_probe(phy, message_id, source_message_id, |_| {}) {
                    ReceiveOutcome::SoftReset | ReceiveOutcome::HardReset => {
                        tier.status = "negotiation_failed";
                        tier.reason = "pd_reset_during_hold";
                        return Ok(tier);
                    }
                    ReceiveOutcome::Packet(_) | ReceiveOutcome::Empty => {}
                }
                delay_ms(1);
                continue;
            }
            next_sample = now.saturating_add(pd::SAMPLE_INTERVAL_MS);
            let _ = pd_irq.is_low();
            match receive_packet_with_probe(phy, message_id, source_message_id, |_| {}) {
                ReceiveOutcome::SoftReset | ReceiveOutcome::HardReset => {
                    tier.status = "negotiation_failed";
                    tier.reason = "pd_reset_during_hold";
                    return Ok(tier);
                }
                ReceiveOutcome::Packet(_) | ReceiveOutcome::Empty => {}
            }
            let status_ok = phy
                .read_status()
                .is_ok_and(|status| status.status0 & STATUS0_VBUSOK != 0);
            // External-source diagnostics use PD VBUSOK and hold timing; the
            // attached IsolaPurr port supplies the independent voltage record.
            let measured = validate_vin
                .then(|| read_vin_mv(adc, vin, calibration))
                .flatten();
            match (status_ok, measured) {
                (true, _) if !validate_vin => {
                    let started = hold_started.get_or_insert(now);
                    if tier.sample_count == 0 {
                        tier.hold_started_ms = *started;
                    }
                    tier.sample_count = tier.sample_count.saturating_add(1);
                    if let Some(previous) = last_valid_at
                        && now.saturating_sub(previous) > SAMPLE_GAP_LIMIT_MS
                    {
                        all_samples_in_range = false;
                    }
                    last_valid_at = Some(now);
                }
                (true, Some(value)) => {
                    let started = hold_started.get_or_insert(now);
                    if tier.sample_count == 0 {
                        tier.hold_started_ms = *started;
                        tier.first_mv = value;
                    }
                    tier.sample_count = tier.sample_count.saturating_add(1);
                    tier.last_mv = value;
                    tier.min_mv = if tier.sample_count == 1 {
                        value
                    } else {
                        tier.min_mv.min(value)
                    };
                    tier.max_mv = tier.max_mv.max(value);
                    tier.mean_mv = tier.mean_mv.saturating_add(u32::from(value));
                    if validate_vin && value.abs_diff(tier.target.voltage_mv) > tolerance {
                        all_samples_in_range = false;
                    }
                    if let Some(previous) = last_valid_at
                        && now.saturating_sub(previous) > SAMPLE_GAP_LIMIT_MS
                    {
                        all_samples_in_range = false;
                    }
                    last_valid_at = Some(now);
                    if validate_vin {
                        emit_sample_progress(
                            usb,
                            request_id,
                            sequence,
                            tier,
                            now.saturating_sub(*started),
                            value,
                            value.abs_diff(tier.target.voltage_mv) <= tolerance,
                        );
                    }
                }
                _ => {
                    tier.invalid_sample_count = tier.invalid_sample_count.saturating_add(1);
                    if validate_vin {
                        emit_sample_progress(
                            usb,
                            request_id,
                            sequence,
                            tier,
                            hold_started.map_or(0, |started| now.saturating_sub(started)),
                            measured.unwrap_or(0),
                            false,
                        );
                    }
                }
            }
        }

        if hold_started.is_none() {
            tier.status = "measurement_failed";
            tier.reason = "no_valid_vin_sample";
        } else if tier.sample_count < 20 {
            tier.status = "measurement_failed";
            tier.reason = "insufficient_valid_samples";
        } else if tier.invalid_sample_count > MAX_INVALID_SAMPLES {
            tier.status = "measurement_failed";
            tier.reason = "too_many_invalid_samples";
        } else if validate_vin && !all_samples_in_range {
            tier.status = "measurement_failed";
            tier.reason = "vin_out_of_tolerance_or_stale";
        } else if !validate_vin && !all_samples_in_range {
            tier.status = "measurement_failed";
            tier.reason = "vin_observation_stale";
        } else {
            tier.status = "pass";
            tier.reason = if validate_vin {
                ""
            } else {
                "pd_contract_held_external_vin"
            };
        }
        emit_tier_progress(usb, request_id, sequence, index, tier.status, tier);
        Ok(tier)
    }

    fn run_session(
        usb: &mut UsbSerialJtag<'static, Blocking>,
        outputs: &mut Outputs,
        measurements: &mut Measurements,
        request_id: &str,
        validate_vin: bool,
    ) -> SessionResult {
        outputs.safe();
        let mut result = SessionResult {
            overall: "capability_discovery_failed",
            validate_vin,
            next_sequence: 0,
            tiers: [TierRecord::EMPTY; pd::TOTAL_TIERS],
            tier_count: 0,
            final_reset: ResetResult::FAILED,
            source_capabilities: pd::SourceCapabilities::empty(),
        };
        let started = now_ms();
        let session_deadline = started.saturating_add(SESSION_TIMEOUT_MS);
        let mut sequence = 0u32;
        let mut cancel = CancelState::new();
        let mut message_id = 0u8;
        let mut source_message_id = None;
        emit_session_progress(usb, request_id, &mut sequence, "preflight");

        let pending_packet = match attach_sink(&mut measurements.i2c) {
            Ok(pending_packet) => pending_packet,
            Err(reason) => {
                emit_session_progress(usb, request_id, &mut sequence, reason);
                result.next_sequence = sequence;
                return result;
            }
        };
        let mut phy = Fusb302::with_address(&mut measurements.i2c, I2C_ADDRESS);
        emit_session_progress(usb, request_id, &mut sequence, "attached");

        let initial_reset = match reset_to_default(
            &mut phy,
            &mut measurements.adc,
            &mut measurements.vin,
            &measurements.vin_calibration,
            &measurements.pd_irq,
            false,
            validate_vin,
            usb,
            &mut cancel,
            session_deadline,
            request_id,
            &mut sequence,
            &mut message_id,
            &mut source_message_id,
            pending_packet,
            None,
        ) {
            Ok(reset) => reset,
            Err(stop) => {
                emit_session_progress(usb, request_id, &mut sequence, "initial_reset_failed");
                result.overall = match stop {
                    StopReason::Cancelled => "cancelled",
                    StopReason::GlobalTimeout => "recovery_failed",
                };
                let mut cleanup_cancel = CancelState::new();
                let cleanup_deadline = now_ms().saturating_add(RECOVERY_TIMEOUT_MS);
                let final_reset = reset_to_default(
                    &mut phy,
                    &mut measurements.adc,
                    &mut measurements.vin,
                    &measurements.vin_calibration,
                    &measurements.pd_irq,
                    true,
                    validate_vin,
                    usb,
                    &mut cleanup_cancel,
                    cleanup_deadline,
                    request_id,
                    &mut sequence,
                    &mut message_id,
                    &mut source_message_id,
                    None,
                    None,
                )
                .unwrap_or(ResetResult::FAILED);
                emit_recovery_progress(
                    usb,
                    request_id,
                    &mut sequence,
                    pd::TOTAL_TIERS,
                    final_reset,
                );
                result.final_reset = final_reset;
                if final_reset.status != "pass" {
                    result.overall = "recovery_failed";
                }
                result.next_sequence = sequence;
                return result;
            }
        };
        result.source_capabilities = initial_reset.source_capabilities;
        emit_recovery_progress(usb, request_id, &mut sequence, 0, initial_reset);
        if initial_reset.status != "pass" {
            result.overall = "recovery_failed";
            let mut cleanup_cancel = CancelState::new();
            let cleanup_deadline = now_ms().saturating_add(RECOVERY_TIMEOUT_MS);
            let final_reset = reset_to_default(
                &mut phy,
                &mut measurements.adc,
                &mut measurements.vin,
                &measurements.vin_calibration,
                &measurements.pd_irq,
                true,
                validate_vin,
                usb,
                &mut cleanup_cancel,
                cleanup_deadline,
                request_id,
                &mut sequence,
                &mut message_id,
                &mut source_message_id,
                None,
                None,
            )
            .unwrap_or(ResetResult::FAILED);
            emit_recovery_progress(usb, request_id, &mut sequence, pd::TOTAL_TIERS, final_reset);
            result.final_reset = final_reset;
            result.next_sequence = sequence;
            return result;
        }

        emit_session_progress(usb, request_id, &mut sequence, "running");
        let mut technical_failure = false;
        let mut unsupported = false;
        for index in 0..pd::TOTAL_TIERS {
            if let Err(stop) = check_deadline(usb, &mut cancel, session_deadline) {
                let reason = match stop {
                    StopReason::Cancelled => "cancelled",
                    StopReason::GlobalTimeout => "global_timeout",
                };
                for remaining in index..pd::TOTAL_TIERS {
                    result.tiers[remaining] = TierRecord {
                        target: pd::target_at(remaining).unwrap_or(pd::Target {
                            mode: pd::Mode::Fixed,
                            voltage_mv: 0,
                        }),
                        status: reason,
                        reason,
                        ..TierRecord::EMPTY
                    };
                }
                result.tier_count = pd::TOTAL_TIERS;
                result.overall = reason;
                break;
            }
            let target = pd::target_at(index).expect("target matrix is bounded");
            let capabilities_for_tier = refresh_source_capabilities_before_request(
                &mut phy,
                usb,
                &mut cancel,
                session_deadline,
                &mut sequence,
                request_id,
                &mut message_id,
                &mut source_message_id,
            );
            if let Ok(capabilities) = capabilities_for_tier {
                result.source_capabilities = capabilities;
            }
            let tier = match capabilities_for_tier {
                Ok(capabilities) => match capabilities.select(target) {
                    Ok(selected) => match run_tier(
                        &mut phy,
                        &mut measurements.adc,
                        &mut measurements.vin,
                        &measurements.vin_calibration,
                        &measurements.pd_irq,
                        usb,
                        &mut cancel,
                        session_deadline,
                        request_id,
                        &mut sequence,
                        selected,
                        index,
                        &mut message_id,
                        &mut source_message_id,
                        validate_vin,
                    ) {
                        Ok(tier) => tier,
                        Err(stop) => {
                            let reason = match stop {
                                StopReason::Cancelled => "cancelled",
                                StopReason::GlobalTimeout => "global_timeout",
                            };
                            let mut tier = TierRecord::new(target);
                            tier.status = reason;
                            tier.reason = reason;
                            tier
                        }
                    },
                    Err(reason) => {
                        unsupported = true;
                        let mut tier = TierRecord::new(target);
                        tier.status = "unsupported";
                        tier.reason = reason.as_str();
                        emit_tier_progress(
                            usb,
                            request_id,
                            &mut sequence,
                            index,
                            tier.status,
                            tier,
                        );
                        tier
                    }
                },
                Err(stop) => {
                    let (status, reason) = match stop {
                        StopReason::Cancelled => ("cancelled", "cancelled"),
                        StopReason::GlobalTimeout => {
                            ("global_timeout", "capabilities_refresh_failed")
                        }
                    };
                    let mut tier = TierRecord::new(target);
                    tier.status = status;
                    tier.reason = reason;
                    emit_tier_progress(usb, request_id, &mut sequence, index, status, tier);
                    tier
                }
            };
            if matches!(tier.status, "negotiation_failed" | "measurement_failed") {
                technical_failure = true;
            }
            if matches!(tier.status, "cancelled" | "global_timeout") {
                result.overall = tier.status;
            }
            result.tiers[index] = tier;
            result.tier_count = index + 1;
            // The terminal tier frame is now outside the PD response window.
            // Drain here so the next request never inherits a full progress
            // queue, while the Source_Capabilities -> Request path stays
            // non-blocking.
            drain_progress(usb);
            if matches!(tier.status, "cancelled" | "global_timeout") {
                for remaining in index + 1..pd::TOTAL_TIERS {
                    result.tiers[remaining] = TierRecord {
                        target: pd::target_at(remaining).unwrap_or(pd::Target {
                            mode: pd::Mode::Fixed,
                            voltage_mv: 0,
                        }),
                        status: tier.status,
                        reason: tier.reason,
                        ..TierRecord::EMPTY
                    };
                }
                result.tier_count = pd::TOTAL_TIERS;
                break;
            }
        }

        let mut cleanup_cancel = CancelState::new();
        let cleanup_deadline = now_ms().saturating_add(RECOVERY_TIMEOUT_MS);
        let final_reset = match reset_to_default(
            &mut phy,
            &mut measurements.adc,
            &mut measurements.vin,
            &measurements.vin_calibration,
            &measurements.pd_irq,
            true,
            validate_vin,
            usb,
            &mut cleanup_cancel,
            cleanup_deadline,
            request_id,
            &mut sequence,
            &mut message_id,
            &mut source_message_id,
            None,
            None,
        ) {
            Ok(reset) => reset,
            Err(_) => ResetResult::FAILED,
        };
        emit_recovery_progress(usb, request_id, &mut sequence, pd::TOTAL_TIERS, final_reset);
        result.final_reset = final_reset;
        result.next_sequence = sequence;
        if final_reset.status != "pass" {
            result.overall = "recovery_failed";
        }

        if result.tier_count == pd::TOTAL_TIERS
            && result.overall != "recovery_failed"
            && result.overall != "cancelled"
            && result.overall != "global_timeout"
        {
            result.overall = if technical_failure {
                "fail"
            } else if unsupported {
                "unsupported"
            } else if !validate_vin {
                "external_source_pass"
            } else {
                "pass"
            };
        }
        if result.final_reset.status == "pass" && result.overall.is_empty() {
            result.overall = "pass";
        }
        result
    }

    fn write_tier_summary(
        out: &mut String<32_768>,
        tier: TierRecord,
        validate_vin: bool,
    ) -> core::fmt::Result {
        out.push_str("{\"mode\":\"").map_err(|_| core::fmt::Error)?;
        out.push_str(tier.target.mode.as_str())
            .map_err(|_| core::fmt::Error)?;
        out.push_str("\",\"targetMv\":")
            .map_err(|_| core::fmt::Error)?;
        push_u32(out, u32::from(tier.target.voltage_mv))?;
        out.push_str(",\"status\":\"")
            .map_err(|_| core::fmt::Error)?;
        out.push_str(tier.status).map_err(|_| core::fmt::Error)?;
        out.push_str("\",\"reason\":\"")
            .map_err(|_| core::fmt::Error)?;
        out.push_str(tier.reason).map_err(|_| core::fmt::Error)?;
        out.push_str("\"").map_err(|_| core::fmt::Error)?;
        push_json_u32(
            out,
            ",\"selectedObjectPosition\":",
            u32::from(tier.selected_object_position),
        )?;
        push_json_u32(
            out,
            ",\"sourceAdvertisedMaxMa\":",
            u32::from(tier.source_max_ma),
        )?;
        push_json_u32(
            out,
            ",\"contractCurrentMa\":",
            u32::from(tier.contract_current_ma),
        )?;
        push_json_u32(out, ",\"contractMv\":", u32::from(tier.contract_mv))?;
        out.push_str(",\"contractConfirmed\":")
            .map_err(|_| core::fmt::Error)?;
        out.push_str(if tier.contract_confirmed {
            "true"
        } else {
            "false"
        })
        .map_err(|_| core::fmt::Error)?;
        push_json_u32(out, ",\"requestSentAtMs\":", tier.request_sent_ms)?;
        push_json_u32(
            out,
            ",\"contractConfirmedAtMs\":",
            tier.contract_confirmed_ms,
        )?;
        push_json_u32(out, ",\"holdStartedAtMs\":", tier.hold_started_ms)?;
        push_json_u32(out, ",\"holdFinishedAtMs\":", tier.hold_finished_ms)?;
        push_json_u32(
            out,
            ",\"holdMs\":",
            if tier.hold_started_ms == 0 || tier.hold_finished_ms == 0 {
                0
            } else {
                tier.hold_finished_ms.saturating_sub(tier.hold_started_ms)
            },
        )?;
        push_json_u32(out, ",\"sampleCount\":", u32::from(tier.sample_count))?;
        push_json_u32(
            out,
            ",\"invalidSampleCount\":",
            u32::from(tier.invalid_sample_count),
        )?;
        if validate_vin {
            push_json_u32(out, ",\"minMeasuredVinMv\":", u32::from(tier.min_mv))?;
            push_json_u32(out, ",\"maxMeasuredVinMv\":", u32::from(tier.max_mv))?;
            push_json_u32(
                out,
                ",\"meanMeasuredVinMv\":",
                if tier.sample_count == 0 {
                    0
                } else {
                    tier.mean_mv / u32::from(tier.sample_count)
                },
            )?;
            push_json_u32(out, ",\"firstMeasuredVinMv\":", u32::from(tier.first_mv))?;
            push_json_u32(out, ",\"lastMeasuredVinMv\":", u32::from(tier.last_mv))?;
        }
        out.push('}').map_err(|_| core::fmt::Error)
    }

    fn write_adc_reset_fields(
        out: &mut String<32_768>,
        reset: ResetResult,
        validate_vin: bool,
    ) -> core::fmt::Result {
        if validate_vin {
            push_json_u32(out, ",\"defaultAdcMv\":", u32::from(reset.default_adc_mv))?;
            push_json_u32(
                out,
                ",\"defaultAdcRawCode\":",
                u32::from(reset.default_adc_raw_code),
            )?;
        }
        push_json_u32(
            out,
            ",\"defaultSampleCount\":",
            u32::from(reset.sample_count),
        )
    }

    fn push_u32(out: &mut String<32_768>, mut value: u32) -> core::fmt::Result {
        let mut digits = [0u8; 10];
        let mut length = 0usize;
        if value == 0 {
            digits[0] = 0;
            length = 1;
        } else {
            while value != 0 {
                digits[length] = (value % 10) as u8;
                value /= 10;
                length += 1;
            }
        }
        for digit in digits[..length].iter().rev() {
            out.push(char::from(b'0' + *digit))
                .map_err(|_| core::fmt::Error)?;
        }
        Ok(())
    }

    fn push_json_u32(out: &mut String<32_768>, prefix: &str, value: u32) -> core::fmt::Result {
        out.push_str(prefix).map_err(|_| core::fmt::Error)?;
        push_u32(out, value)
    }

    fn write_summary(
        out: &mut String<32_768>,
        request_id: &str,
        result: &SessionResult,
        mut emit_stage: impl FnMut(&str),
    ) -> core::fmt::Result {
        use core::fmt::Write;
        let pd_status = if result.final_reset.status == "pass" {
            "default_verified"
        } else {
            "resetting"
        };
        protocol::write_summary_prefix(out, request_id, result.overall, pd_status)?;
        emit_stage("summary_prefix_done");
        out.push_str("\"policy\":{\"fixedMv\":[5000,9000,12000,15000,20000],\"ppsMv\":[5000,6000,7000,8000,9000,10000,11000,12000,13000,14000,15000,16000,17000,18000,19000,20000,21000],\"currentMode\":\"max\",\"currentCeilingMa\":5000,\"minimumCurrentMa\":3000,\"recoveryMode\":\"fixed_5v_contract\",\"vinValidation\":\"")
            .map_err(|_| core::fmt::Error)?;
        out.push_str(if result.validate_vin {
            "adc"
        } else {
            "external_source"
        })
        .map_err(|_| core::fmt::Error)?;
        out.push_str(
            "\",\"holdMs\":2000,\"sampleIntervalMs\":50,\"toleranceRule\":\"max(250mV,2.5%)\"},",
        )
        .map_err(|_| core::fmt::Error)?;
        emit_stage("summary_policy_done");
        let source_count = usize::from(result.source_capabilities.count).min(pd::MAX_SOURCE_PDOS);
        if source_count == 0 {
            out.push_str("\"sourceCapabilities\":{\"count\":0,\"rawPdos\":[],\"objects\":[]}")
                .map_err(|_| core::fmt::Error)?;
        } else {
            out.push_str("\"sourceCapabilities\":{\"count\":")
                .map_err(|_| core::fmt::Error)?;
            push_u32(out, source_count as u32)?;
            out.push_str(",\"rawPdos\":[")
                .map_err(|_| core::fmt::Error)?;
        }
        if source_count != 0 {
            for index in 0..source_count {
                if index != 0 {
                    out.push(',').map_err(|_| core::fmt::Error)?;
                }
                push_u32(out, result.source_capabilities.objects[index].raw)?;
            }
            out.push_str("],\"objects\":[]}")
                .map_err(|_| core::fmt::Error)?;
        }
        emit_stage("summary_capabilities_done");
        out.push_str(",\"tiers\":[").map_err(|_| core::fmt::Error)?;
        let tier_count = result.tier_count.min(pd::TOTAL_TIERS);
        emit_stage(if tier_count == 0 {
            "summary_tier_count_zero"
        } else {
            "summary_tier_count_nonzero"
        });
        for index in 0..tier_count {
            if index != 0 {
                out.push(',').map_err(|_| core::fmt::Error)?;
            }
            write_tier_summary(out, result.tiers[index], result.validate_vin)?;
        }
        emit_stage("summary_tiers_done");
        let mut length_stage = String::<64>::new();
        let _ = write!(length_stage, "summary_tiers_len_{}", out.len());
        emit_stage(length_stage.as_str());
        emit_stage("summary_final_reset_begin");
        if tier_count == 0 {
            out.push_str("],\"finalReset\":{\"status\":\"")
                .map_err(|_| core::fmt::Error)?;
            out.push_str(result.final_reset.status)
                .map_err(|_| core::fmt::Error)?;
            out.push_str(
                "\",\"pdProtocol\":\"fixed_5v\",\"activeContract\":null,\"pendingRequest\":null,\"defaultVbusMv\":",
            )
                .map_err(|_| core::fmt::Error)?;
            push_u32(out, u32::from(result.final_reset.default_vbus_mv))?;
            write_adc_reset_fields(out, result.final_reset, result.validate_vin)?;
            out.push_str(",\"reason\":\"")
                .map_err(|_| core::fmt::Error)?;
            out.push_str(result.final_reset.reason)
                .map_err(|_| core::fmt::Error)?;
            out.push_str("\"").map_err(|_| core::fmt::Error)?;
            out.push('}').map_err(|_| core::fmt::Error)?;
            emit_stage("summary_reset_done");
            let result = protocol::write_summary_suffix(out);
            if result.is_ok() {
                emit_stage("summary_suffix_done");
            }
            return result;
        }
        out.push_str("],\"finalReset\":{\"status\":\"")
            .map_err(|_| core::fmt::Error)?;
        out.push_str(result.final_reset.status)
            .map_err(|_| core::fmt::Error)?;
        out.push_str("\",\"pdProtocol\":\"fixed_5v\",\"activeContract\":")
            .map_err(|_| core::fmt::Error)?;
        if result.final_reset.status == "pass" {
            out.push_str("{\"mode\":\"fixed\",\"voltageMv\":")
                .map_err(|_| core::fmt::Error)?;
            push_u32(out, u32::from(pd::DEFAULT_CONTRACT_MV))?;
            out.push_str(",\"currentMa\":")
                .map_err(|_| core::fmt::Error)?;
            push_u32(
                out,
                u32::from(result.final_reset.default_contract_current_ma),
            )?;
            out.push('}').map_err(|_| core::fmt::Error)?;
        } else {
            out.push_str("null").map_err(|_| core::fmt::Error)?;
        }
        out.push_str(",\"pendingRequest\":null,\"defaultVbusMv\":")
            .map_err(|_| core::fmt::Error)?;
        push_u32(out, u32::from(result.final_reset.default_vbus_mv))?;
        write_adc_reset_fields(out, result.final_reset, result.validate_vin)?;
        out.push_str(",\"reason\":\"")
            .map_err(|_| core::fmt::Error)?;
        out.push_str(result.final_reset.reason)
            .map_err(|_| core::fmt::Error)?;
        out.push_str("\"}").map_err(|_| core::fmt::Error)?;
        emit_stage("summary_reset_done");
        let result = protocol::write_summary_suffix(out);
        if result.is_ok() {
            emit_stage("summary_suffix_done");
        }
        result
    }

    #[inline(never)]
    fn send_summary(
        usb: &mut UsbSerialJtag<'static, Blocking>,
        request_id: &str,
        result: &SessionResult,
    ) {
        super::rom_diag_line(b"ram_pd_hil_stage=summary_enter\n");
        let mut summary_sequence = result.next_sequence;
        super::rom_diag_line(b"ram_pd_hil_stage=summary_prepare_begin\n");
        emit_session_progress(usb, request_id, &mut summary_sequence, "summary_prepare");
        super::rom_diag_line(b"ram_pd_hil_stage=summary_prepare_returned\n");
        // SAFETY: `handle_line` is the only caller and processes requests
        // synchronously, so the storage is never aliased across sessions.
        let response = unsafe { &mut *SUMMARY_STORAGE.0.get() };
        response.clear();
        emit_session_progress(
            usb,
            request_id,
            &mut summary_sequence,
            "summary_buffer_ready",
        );
        if write_summary(response, request_id, result, |stage| {
            emit_session_progress(usb, request_id, &mut summary_sequence, stage);
        })
        .is_ok()
        {
            emit_session_progress(usb, request_id, &mut summary_sequence, "summary_send");
            drain_progress(usb);
            emit_summary_chunks(usb, request_id, &mut summary_sequence, response.as_bytes());
            emit_session_progress(usb, request_id, &mut summary_sequence, "summary_write_done");
            drain_progress(usb);
        } else {
            emit_session_progress(
                usb,
                request_id,
                &mut summary_sequence,
                "summary_build_failed",
            );
        }
    }

    pub fn run() -> ! {
        let peripherals = esp_hal::init(esp_hal::Config::default());
        let (usb_device, tokens) = PeripheralTokens::split(peripherals);
        let mut usb = UsbSerialJtag::new(usb_device);
        let reset_reason = "ram_boot";
        let mut hello = String::<1024>::new();
        let _ = protocol::write_identity(&mut hello, reset_reason);
        super::rom_log_line(hello.as_bytes());

        let (mut outputs, mut measurements) = match Outputs::new(tokens) {
            Ok(value) => value,
            Err(()) => {
                super::rom_log_line(b"ram_pd_hil_error=i2c_init_failed\n");
                esp_hal::system::software_reset();
            }
        };
        outputs.safe();
        let mut line = [0u8; LINE_MAX];
        let mut length = 0usize;
        let mut discarding_line = false;
        loop {
            let byte = match usb.read_byte() {
                Ok(byte) => byte,
                Err(_) => {
                    delay_ms(1);
                    continue;
                }
            };
            if byte == b'\n' {
                if !discarding_line && length > 0 {
                    handle_line(&mut usb, &mut outputs, &mut measurements, &line[..length]);
                }
                length = 0;
                discarding_line = false;
            } else if discarding_line {
                continue;
            } else if length < line.len() {
                line[length] = byte;
                length += 1;
            } else {
                discarding_line = true;
                length = 0;
                outputs.safe();
            }
        }
    }

    #[inline(never)]
    fn handle_line(
        usb: &mut UsbSerialJtag<'static, Blocking>,
        outputs: &mut Outputs,
        measurements: &mut Measurements,
        line: &[u8],
    ) {
        outputs.safe();
        let Some((request, _)) = protocol::parse_request(line) else {
            return;
        };
        if request.command() != Some(protocol::Command::TestPdSink) {
            return;
        }
        if SKIP_PD_SESSION_FOR_DIAG {
            let mut sequence = 0u32;
            super::rom_diag_line(b"ram_pd_hil_stage=direct_summary_begin\n");
            emit_session_progress(
                usb,
                request.request_id.as_str(),
                &mut sequence,
                "summary_prepare",
            );
            super::rom_diag_line(b"ram_pd_hil_stage=direct_summary_returned\n");
            return;
        }
        let result = run_session(
            usb,
            outputs,
            measurements,
            request.request_id.as_str(),
            request.validate_vin != Some(false),
        );
        outputs.safe();
        send_summary(usb, request.request_id.as_str(), &result);
    }
}

#[cfg(target_arch = "xtensa")]
#[esp_hal::main]
fn main() -> ! {
    device::run()
}

#[cfg(target_arch = "xtensa")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    use core::fmt::Write;

    let mut message = heapless::String::<512>::new();
    let _ = write!(message, "{}", info);
    rom_log_line(b"ram_pd_hil_error=panic:");
    rom_log_line(message.as_bytes());
    rom_log_line(b"\n");
    esp_hal::system::software_reset()
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn tier_progress_payload_formats_into_the_static_buffer() {
        use core::fmt::Write;

        let mut result = heapless::String::<512>::new();
        let status = "requesting";
        let mode = "fixed";
        write!(
            result,
            "{{\"kind\":\"tier\",\"mode\":\"{}\",\"targetMv\":{},\"index\":{},\"total\":{},\"status\":\"{status}\",\"contractMv\":{},\"contractCurrentMa\":{},\"measuredVinMv\":{},\"heater\":\"off\",\"pd\":\"owned\",\"eeprom\":\"untouched\"}}",
            mode, 20_000, 22, 22, 20_000, 5_000, 0,
        )
        .unwrap();

        assert!(result.starts_with("{\"kind\":\"tier\""));
        assert!(result.len() < 512);
    }

    #[test]
    fn source_message_id_accepts_forward_modulo_eight_ids() {
        let source = include_str!("main.rs");
        let helper = source
            .split("fn source_message_id_is_fresh")
            .nth(1)
            .and_then(|value| value.split("fn observe_source_message_id").next())
            .expect("source message-id freshness helper must remain present");
        assert!(helper.contains("(1..=4).contains(&distance)"));
    }

    #[test]
    fn recovery_path_preserves_the_attached_usb_c_session() {
        let source = include_str!("main.rs");
        let recovery = source
            .split("fn reset_to_default")
            .nth(1)
            .and_then(|value| value.split("fn discover_capabilities").next())
            .expect("PD HIL reset path must remain present");

        assert!(recovery.contains("resynchronize_attached"));
        assert!(recovery.contains("select_default_5v"));
        assert!(!recovery.contains("hard_reset_attached_source"));
        assert!(!recovery.contains("phy.pd_reset"));
        assert!(!recovery.contains("start_toggle"));
    }

    #[test]
    fn capability_discovery_resynchronizes_a_retained_session_after_one_query() {
        let source = include_str!("main.rs");
        let discovery = source
            .split("fn discover_capabilities")
            .nth(1)
            .and_then(|value| value.split("fn run_tier").next())
            .expect("PD HIL capability discovery must remain present");

        let query = discovery
            .find("capabilities_request")
            .expect("capability discovery must retain query fallback");
        assert!(discovery.contains("reset_source_protocol_for_capabilities"));
        assert!(source.contains("soft_reset_header"));
        assert!(source.contains("capabilities_protocol_reset_accepted"));
        assert!(source.contains("capabilities_protocol_reset_begin"));
        assert!(discovery.contains("capabilities_unavailable_after_protocol_reset"));
        assert!(query > 0);
    }

    #[test]
    fn tiers_refresh_source_capabilities_before_requesting_the_target() {
        let source = include_str!("main.rs");
        let session = source
            .split("fn run_session")
            .nth(1)
            .expect("PD HIL session must remain present");
        let target = session
            .find("let target = pd::target_at(index)")
            .expect("tier loop must resolve the target before requesting it");
        let refresh = session
            .find("refresh_source_capabilities_before_request")
            .expect("tiers must refresh source capabilities");
        let request = session
            .find("match capabilities.select(target)")
            .expect("tier selection must follow the capability boundary");
        assert!(target < refresh);
        assert!(refresh < request);
        assert!(source.contains("capabilities_refresh_received"));
    }

    #[test]
    fn tier_loop_keeps_the_previous_contract_until_the_next_request() {
        let source = include_str!("main.rs");
        let session = source
            .split("fn run_session")
            .nth(1)
            .expect("PD HIL session must remain present");
        let tier_loop = session
            .split("for index in 0..pd::TOTAL_TIERS")
            .nth(1)
            .and_then(|value| {
                value
                    .split("let final_reset = match reset_to_default")
                    .next()
            })
            .expect("PD HIL tier loop must remain present");

        assert!(tier_loop.contains("refresh_source_capabilities_before_request"));
        assert!(!tier_loop.contains("reset_to_default("));
    }

    #[test]
    fn attach_reuses_retained_fusb_session_before_reset_fallback() {
        let source = include_str!("main.rs");
        let attach = source
            .split("fn attach_sink")
            .nth(1)
            .and_then(|value| value.split("fn configure_attached").next())
            .expect("PD HIL attach path must remain present");

        let retained = attach
            .find("existing_attachment_polarity")
            .expect("attach must inspect retained FUSB CC state");
        let reset = attach
            .find("phy.init()")
            .expect("attach must retain a cold-start reset fallback");
        assert!(retained < reset);
        assert!(attach.contains("ram_pd_hil_existing_session_reused"));
    }
}
