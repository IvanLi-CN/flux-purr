#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

mod protocol;

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
mod device {
    use super::protocol::{self, DisplayPattern};
    use embedded_hal::pwm::SetDutyCycle;
    use embedded_hal::spi::SpiBus;
    use esp_hal::{
        Blocking,
        analog::adc::{Adc, AdcConfig, AdcPin, Attenuation},
        gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
        i2c::master::{Config as I2cConfig, I2c},
        mcpwm::{
            McPwm, PeripheralClockConfig,
            operator::{PwmPin, PwmPinConfig},
            timer::PwmWorkingMode,
        },
        spi::{
            Mode as SpiMode,
            master::{Config as SpiConfig, Spi},
        },
        time::Rate,
        usb_serial_jtag::UsbSerialJtag,
    };
    use heapless::String;

    const LINE_MAX: usize = 512;
    const DISPLAY_FRAME_BYTES: usize = 160 * 50 * 2;
    const MCPWM_PERIPHERAL_CLOCK_HZ: u32 = 40_000_000;
    const FAN_PWM_PERIOD_TICKS: u16 = 99;
    const FAN_PWM_FREQUENCY_HZ: u32 = 25_000;
    // Keep the frame out of rodata so the RAM ELF's executable segment stays below the next alignment boundary.
    #[unsafe(link_section = ".data")]
    static CALIBRATION_FRAME: [u8; DISPLAY_FRAME_BYTES] =
        *include_bytes!("../assets/calibration.panel.rgb565be.bin");

    fn delay_ms(milliseconds: u32) {
        esp_hal::rom::ets_delay_us(milliseconds.saturating_mul(1_000));
    }

    type AdcDriver = Adc<'static, esp_hal::peripherals::ADC1<'static>, Blocking>;
    type AdcPin1 =
        AdcPin<esp_hal::peripherals::GPIO1<'static>, esp_hal::peripherals::ADC1<'static>>;
    type AdcPin2 =
        AdcPin<esp_hal::peripherals::GPIO2<'static>, esp_hal::peripherals::ADC1<'static>>;
    type I2cBus = I2c<'static, Blocking>;
    type DisplayBus = Spi<'static, Blocking>;

    struct PeripheralTokens {
        gpio0: esp_hal::peripherals::GPIO0<'static>,
        gpio16: esp_hal::peripherals::GPIO16<'static>,
        gpio17: esp_hal::peripherals::GPIO17<'static>,
        gpio18: esp_hal::peripherals::GPIO18<'static>,
        gpio21: esp_hal::peripherals::GPIO21<'static>,
        gpio1: esp_hal::peripherals::GPIO1<'static>,
        gpio2: esp_hal::peripherals::GPIO2<'static>,
        adc1: esp_hal::peripherals::ADC1<'static>,
        i2c0: esp_hal::peripherals::I2C0<'static>,
        gpio8: esp_hal::peripherals::GPIO8<'static>,
        gpio9: esp_hal::peripherals::GPIO9<'static>,
        gpio47: esp_hal::peripherals::GPIO47<'static>,
        gpio35: esp_hal::peripherals::GPIO35<'static>,
        gpio36: esp_hal::peripherals::GPIO36<'static>,
        gpio48: esp_hal::peripherals::GPIO48<'static>,
        gpio13: esp_hal::peripherals::GPIO13<'static>,
        gpio14: esp_hal::peripherals::GPIO14<'static>,
        gpio39: esp_hal::peripherals::GPIO39<'static>,
        gpio38: esp_hal::peripherals::GPIO38<'static>,
        gpio37: esp_hal::peripherals::GPIO37<'static>,
        spi2: esp_hal::peripherals::SPI2<'static>,
        gpio10: esp_hal::peripherals::GPIO10<'static>,
        gpio11: esp_hal::peripherals::GPIO11<'static>,
        gpio12: esp_hal::peripherals::GPIO12<'static>,
        gpio15: esp_hal::peripherals::GPIO15<'static>,
        mcpwm0: esp_hal::peripherals::MCPWM0<'static>,
    }

    impl PeripheralTokens {
        fn split(
            peripherals: esp_hal::peripherals::Peripherals,
        ) -> (esp_hal::peripherals::USB_DEVICE<'static>, Self) {
            let esp_hal::peripherals::Peripherals {
                USB_DEVICE,
                GPIO0: gpio0,
                GPIO16: gpio16,
                GPIO17: gpio17,
                GPIO18: gpio18,
                GPIO21: gpio21,
                GPIO1: gpio1,
                GPIO2: gpio2,
                ADC1: adc1,
                I2C0: i2c0,
                GPIO8: gpio8,
                GPIO9: gpio9,
                GPIO47: gpio47,
                GPIO35: gpio35,
                GPIO36: gpio36,
                GPIO48: gpio48,
                GPIO13: gpio13,
                GPIO14: gpio14,
                GPIO39: gpio39,
                GPIO38: gpio38,
                GPIO37: gpio37,
                SPI2: spi2,
                GPIO10: gpio10,
                GPIO11: gpio11,
                GPIO12: gpio12,
                GPIO15: gpio15,
                MCPWM0: mcpwm0,
                ..
            } = peripherals;
            (
                USB_DEVICE,
                Self {
                    gpio0,
                    gpio16,
                    gpio17,
                    gpio18,
                    gpio21,
                    gpio1,
                    gpio2,
                    adc1,
                    i2c0,
                    gpio8,
                    gpio9,
                    gpio47,
                    gpio35,
                    gpio36,
                    gpio48,
                    gpio13,
                    gpio14,
                    gpio39,
                    gpio38,
                    gpio37,
                    spi2,
                    gpio10,
                    gpio11,
                    gpio12,
                    gpio15,
                    mcpwm0,
                },
            )
        }
    }

    pub struct Outputs {
        heater: Output<'static>,
        fan: Output<'static>,
        fan_pwm: PwmPin<'static, esp_hal::peripherals::MCPWM0<'static>, 0, true>,
        buzzer: Output<'static>,
        backlight: Output<'static>,
        display_reset: Output<'static>,
        display_cs: Output<'static>,
        display_dc: Output<'static>,
        display: DisplayBus,
        red: Output<'static>,
        green: Output<'static>,
        blue: Output<'static>,
    }

    pub struct Inputs {
        center: Input<'static>,
        right: Input<'static>,
        down: Input<'static>,
        left: Input<'static>,
        up: Input<'static>,
    }

    pub struct Measurements {
        adc: AdcDriver,
        vin: AdcPin1,
        rtd: AdcPin2,
        i2c: I2cBus,
    }

    enum OutputInitError {
        I2c,
        Display,
    }

    impl Outputs {
        fn new(tokens: PeripheralTokens) -> Result<(Self, Inputs, Measurements), OutputInitError> {
            let input_config = InputConfig::default().with_pull(Pull::Up);
            let inputs = Inputs {
                center: Input::new(tokens.gpio0, input_config),
                right: Input::new(tokens.gpio16, input_config),
                down: Input::new(tokens.gpio17, input_config),
                left: Input::new(tokens.gpio18, input_config),
                up: Input::new(tokens.gpio21, input_config),
            };
            let mut adc_config = AdcConfig::new();
            let vin = adc_config.enable_pin(tokens.gpio1, Attenuation::_11dB);
            let rtd = adc_config.enable_pin(tokens.gpio2, Attenuation::_11dB);
            let adc = Adc::new(tokens.adc1, adc_config);
            let i2c = I2c::new(
                tokens.i2c0,
                I2cConfig::default().with_frequency(Rate::from_khz(400)),
            )
            .map_err(|_| OutputInitError::I2c)?
            .with_sda(tokens.gpio8)
            .with_scl(tokens.gpio9);
            let display = Spi::new(
                tokens.spi2,
                SpiConfig::default()
                    .with_frequency(Rate::from_mhz(40))
                    .with_mode(SpiMode::_0),
            )
            .map_err(|_| OutputInitError::Display)?
            .with_sck(tokens.gpio12)
            .with_mosi(tokens.gpio11);
            let pwm_clock =
                PeripheralClockConfig::with_frequency(Rate::from_hz(MCPWM_PERIPHERAL_CLOCK_HZ))
                    .expect("failed to derive RAM MCPWM peripheral clock");
            let mcpwm = McPwm::new(tokens.mcpwm0, pwm_clock);
            let esp_hal::mcpwm::McPwm {
                mut timer0,
                mut operator0,
                ..
            } = mcpwm;
            operator0.set_timer(&timer0);
            let mut fan_pwm = operator0.with_pin_a(tokens.gpio36, PwmPinConfig::UP_ACTIVE_HIGH);
            let fan_timer = pwm_clock
                .timer_clock_with_frequency(
                    FAN_PWM_PERIOD_TICKS,
                    PwmWorkingMode::Increase,
                    Rate::from_hz(FAN_PWM_FREQUENCY_HZ),
                )
                .expect("failed to derive RAM fan PWM timer clock");
            timer0.start(fan_timer);
            let _ = fan_pwm.set_duty_cycle_percent(0);
            let outputs = Self {
                heater: Output::new(tokens.gpio47, Level::Low, OutputConfig::default()),
                fan: Output::new(tokens.gpio35, Level::Low, OutputConfig::default()),
                fan_pwm,
                buzzer: Output::new(tokens.gpio48, Level::Low, OutputConfig::default()),
                backlight: Output::new(tokens.gpio13, Level::High, OutputConfig::default()),
                display_reset: Output::new(tokens.gpio14, Level::High, OutputConfig::default()),
                display_cs: Output::new(tokens.gpio15, Level::High, OutputConfig::default()),
                display_dc: Output::new(tokens.gpio10, Level::Low, OutputConfig::default()),
                display,
                red: Output::new(tokens.gpio39, Level::High, OutputConfig::default()),
                green: Output::new(tokens.gpio38, Level::High, OutputConfig::default()),
                blue: Output::new(tokens.gpio37, Level::High, OutputConfig::default()),
            };
            Ok((outputs, inputs, Measurements { adc, vin, rtd, i2c }))
        }

        fn safe(&mut self) {
            self.heater.set_low();
            self.fan.set_low();
            let _ = self.fan_pwm.set_duty_cycle_percent(0);
            self.buzzer.set_low();
            self.backlight.set_high();
            self.display_reset.set_high();
            let _ = self.set_display_cs(Level::High);
            let _ = self.set_display_dc(Level::Low);
            self.red.set_high();
            self.green.set_high();
            self.blue.set_high();
        }

        fn set_display_cs(&mut self, level: Level) -> bool {
            if SpiBus::flush(&mut self.display).is_err() {
                return false;
            }
            self.display_cs.set_level(level);
            true
        }

        fn set_display_dc(&mut self, level: Level) -> bool {
            if SpiBus::flush(&mut self.display).is_err() {
                return false;
            }
            self.display_dc.set_level(level);
            true
        }

        fn rgb(&mut self, red: bool, green: bool, blue: bool) {
            self.red
                .set_level(if red { Level::Low } else { Level::High });
            self.green
                .set_level(if green { Level::Low } else { Level::High });
            self.blue
                .set_level(if blue { Level::Low } else { Level::High });
        }

        #[inline(never)]
        fn display_preview(&mut self, pattern: DisplayPattern) -> bool {
            self.display_reset.set_low();
            delay_ms(10);
            self.display_reset.set_high();
            delay_ms(120);
            // Match the pinned gc9d01 driver's panel_160x50 initialization.
            for command in [0xfe, 0xef] {
                if !self.command(command, &[]) {
                    return false;
                }
            }
            for register in 0x80..=0x8f {
                if !self.command(register, &[0xff]) {
                    return false;
                }
            }
            static INIT: &[(u8, &[u8])] = &[
                (0x3a, &[0x05]),
                (0x7e, &[0x30]),
                (0x74, &[0x05, 0x4d, 0x00, 0x00, 0x01, 0x00, 0x00]),
                (0x98, &[0x3e]),
                (0x99, &[0x3e]),
                (0xb5, &[0x0d, 0x0d]),
                (0x60, &[0x38, 0x09, 0x1e, 0x7a]),
                (0x63, &[0x38, 0xae, 0x1e, 0x7a]),
                (0x64, &[0x38, 0x0b, 0x70, 0xab, 0x1e, 0x7a]),
                (0x66, &[0x38, 0x0f, 0x70, 0xaf, 0x1e, 0x7a]),
                (0x68, &[0x00, 0x08, 0x07, 0x00, 0x07, 0x55, 0x6a]),
                (0x6a, &[0x00, 0x00]),
                (0x6c, &[0x22, 0x02, 0x22, 0x02, 0x22, 0x22, 0x50]),
                (
                    0x6e,
                    &[
                        0x00, 0x00, 0x00, 0x02, 0x14, 0x12, 0x0c, 0x0a, 0x1e, 0x1d, 0x08, 0x00,
                        0x16, 0x15, 0x00, 0x00, 0x00, 0x00, 0x15, 0x16, 0x00, 0x07, 0x1d, 0x1e,
                        0x09, 0x0b, 0x11, 0x13, 0x01, 0x00, 0x00, 0x00,
                    ],
                ),
                (0xbf, &[0x00]),
                (0xf9, &[0x40]),
                (0x9b, &[0x3b]),
                (0x93, &[0x33, 0x7f, 0x00]),
                (0x91, &[0x0e, 0x09]),
                (0x70, &[0x04, 0x02, 0x0d, 0x04, 0x02, 0x0d]),
                (0x71, &[0x04, 0x02, 0x0d]),
                (0xc3, &[0x26]),
                (0xc4, &[0x26]),
                (0xc9, &[0x1c]),
                (0xf0, &[0x02, 0x03, 0x0a, 0x06, 0x00, 0x1a]),
                (0xf2, &[0x02, 0x03, 0x0a, 0x06, 0x00, 0x1a]),
                (0xf1, &[0x38, 0x78, 0x1b, 0x2e, 0x2f, 0xc8]),
                (0xf3, &[0x38, 0x74, 0x12, 0x2e, 0x2f, 0xdf]),
                (0xec, &[0x00]),
                (0x36, &[0x00]),
                (0x2a, &[0x00, 0x0f, 0x00, 0x40]),
                (0x2b, &[0x00, 0x00, 0x00, 0x9f]),
            ];
            for &(command, data) in INIT {
                if !self.command(command, data) {
                    return false;
                }
            }
            if !self.command(0x11, &[]) {
                return false;
            }
            delay_ms(200);
            if !self.command(0x29, &[]) || !self.command(0x2c, &[]) {
                return false;
            }
            delay_ms(100);
            if !self.set_display_dc(Level::High) || !self.set_display_cs(Level::Low) {
                let _ = self.set_display_cs(Level::High);
                return false;
            }
            let mut pixels = [0u8; 128];
            let frame: &[u8] = match pattern {
                DisplayPattern::Calibration => &CALIBRATION_FRAME,
                DisplayPattern::Solid(rgb565) => {
                    for chunk in pixels.chunks_exact_mut(2) {
                        chunk.copy_from_slice(&rgb565);
                    }
                    &pixels
                }
            };
            for index in 0..(DISPLAY_FRAME_BYTES / pixels.len()) {
                let chunk = if matches!(pattern, DisplayPattern::Calibration) {
                    &frame[index * pixels.len()..(index + 1) * pixels.len()]
                } else {
                    &frame[..]
                };
                if SpiBus::write(&mut self.display, chunk).is_err() {
                    let _ = self.set_display_cs(Level::High);
                    return false;
                }
            }
            if SpiBus::flush(&mut self.display).is_err() {
                let _ = self.set_display_cs(Level::High);
                return false;
            }
            self.set_display_cs(Level::High)
        }

        fn command(&mut self, command: u8, data: &[u8]) -> bool {
            if !self.set_display_cs(Level::Low) || !self.set_display_dc(Level::Low) {
                let _ = self.set_display_cs(Level::High);
                return false;
            }
            if SpiBus::write(&mut self.display, &[command])
                .and_then(|_| SpiBus::flush(&mut self.display))
                .is_err()
            {
                let _ = self.set_display_cs(Level::High);
                return false;
            }
            if !data.is_empty() {
                if !self.set_display_dc(Level::High) {
                    let _ = self.set_display_cs(Level::High);
                    return false;
                }
                if SpiBus::write(&mut self.display, data)
                    .and_then(|_| SpiBus::flush(&mut self.display))
                    .is_err()
                {
                    let _ = self.set_display_cs(Level::High);
                    return false;
                }
            }
            self.set_display_cs(Level::High)
        }
    }

    pub async fn run() -> ! {
        let peripherals = esp_hal::init(esp_hal::Config::default());
        let (usb_device, tokens) = PeripheralTokens::split(peripherals);
        let mut usb = UsbSerialJtag::new(usb_device);
        let mut hello = String::<1024>::new();
        let _ = protocol::write_identity(&mut hello);
        super::rom_log_line(hello.as_bytes());
        let (mut outputs, mut inputs, mut measurements) = match Outputs::new(tokens) {
            Ok(value) => value,
            Err(OutputInitError::I2c) => {
                super::rom_log_line(b"ram_error=outputs_i2c_init_failed\n");
                esp_hal::system::software_reset();
            }
            Err(OutputInitError::Display) => {
                super::rom_log_line(b"ram_error=outputs_display_init_failed\n");
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
                    handle_line(
                        &mut outputs,
                        &mut inputs,
                        &mut measurements,
                        &line[..length],
                    );
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

    fn handle_line(
        outputs: &mut Outputs,
        inputs: &mut Inputs,
        measurements: &mut Measurements,
        line: &[u8],
    ) {
        outputs.safe();
        let Some((request, _)) = protocol::parse_request(line) else {
            return;
        };
        let Some(_command) = request.command() else {
            return;
        };
        let op = request.op.as_bytes();
        let display_pattern = match _command {
            protocol::Command::PreviewDisplay => request
                .display_pattern()
                .expect("validated display preview pattern"),
            protocol::Command::PreviewFrontpanel => DisplayPattern::Solid([0x07, 0xe0]),
            _ => DisplayPattern::Calibration,
        };
        let (ok, detail) = if protocol::equal_literal(op, b"preview_display")
            || protocol::equal_literal(op, b"preview_frontpanel")
        {
            outputs.backlight.set_low();
            let ok = outputs.display_preview(display_pattern);
            if ok {
                outputs.rgb(false, true, true);
                (true, "display_preview_ready")
            } else {
                outputs.safe();
                (false, "display_preview_failed")
            }
        } else if protocol::equal_literal(op, b"preview_status_light") {
            outputs.rgb(false, true, false);
            (true, "status_light_preview_ready")
        } else if protocol::equal_literal(op, b"test_buttons") {
            let _states = [
                inputs.center.is_low(),
                inputs.right.is_low(),
                inputs.down.is_low(),
                inputs.left.is_low(),
                inputs.up.is_low(),
            ];
            (true, "buttons_read_only_ready")
        } else if protocol::equal_literal(op, b"test_adc") {
            let mut vin_ok = false;
            for _ in 0..1_000 {
                match measurements.adc.read_oneshot(&mut measurements.vin) {
                    Ok(_) => {
                        vin_ok = true;
                        break;
                    }
                    Err(nb::Error::WouldBlock) => delay_ms(1),
                    Err(nb::Error::Other(_)) => break,
                }
            }
            let mut rtd_ok = false;
            for _ in 0..1_000 {
                match measurements.adc.read_oneshot(&mut measurements.rtd) {
                    Ok(_) => {
                        rtd_ok = true;
                        break;
                    }
                    Err(nb::Error::WouldBlock) => delay_ms(1),
                    Err(nb::Error::Other(_)) => break,
                }
            }
            (
                vin_ok && rtd_ok,
                if vin_ok && rtd_ok {
                    "adc_read_only_ready"
                } else {
                    "adc_read_failed"
                },
            )
        } else if protocol::equal_literal(op, b"test_i2c") {
            let address = request.address.unwrap_or(0x22);
            let register = request
                .register
                .unwrap_or(if address == 0x22 { 0x09 } else { 0x00 });
            let mut value = [0u8; 1];
            let ok = measurements
                .i2c
                .write_read(address, &[register], &mut value)
                .is_ok();
            (
                ok,
                if ok {
                    "i2c_identification_read_only_ready"
                } else {
                    "i2c_identification_read_failed"
                },
            )
        } else if protocol::equal_literal(op, b"test_rgb") {
            outputs.rgb(true, false, false);
            delay_ms(100);
            outputs.rgb(false, true, false);
            delay_ms(100);
            outputs.rgb(false, false, true);
            delay_ms(100);
            outputs.rgb(false, false, false);
            (true, "rgb_ready")
        } else if protocol::equal_literal(op, b"test_buzzer") {
            outputs.buzzer.set_high();
            delay_ms(20);
            outputs.buzzer.set_low();
            (true, "buzzer_ready")
        } else if protocol::equal_literal(op, b"test_fan") {
            let _ = outputs.fan_pwm.set_duty_cycle_percent(50);
            outputs.fan.set_high();
            delay_ms(500);
            let _ = outputs.fan_pwm.set_duty_cycle_percent(0);
            outputs.fan.set_low();
            (true, "fan_ready")
        } else {
            outputs.safe();
            (true, "safe_exit")
        };
        outputs.heater.set_low();
        if !ok {
            outputs.safe();
        }
        let mut response = String::<512>::new();
        let response_write_ok = protocol::write_response(
            &mut response,
            request.request_id.as_str(),
            request.op.as_str(),
            ok,
            detail,
        )
        .is_ok();
        if response_write_ok && !response.is_empty() {
            super::rom_log_line(response.as_bytes());
        }
        if protocol::equal_literal(op, b"exit") {
            esp_hal::system::software_reset();
        }
    }
}

#[cfg(target_arch = "xtensa")]
#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) -> ! {
    device::run().await
}

#[cfg(target_arch = "xtensa")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    rom_log_line(b"ram_error=panic\n");
    esp_hal::system::software_reset()
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {}

#[cfg(test)]
mod tests {
    use super::protocol::{Command, parse_request};

    #[test]
    fn ram_frame_is_not_a_product_request() {
        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r1","op":"preview_display","capability":"preview_display"}"#,
        )
        .unwrap();
        assert_eq!(request.command(), Some(Command::PreviewDisplay));
    }
}
