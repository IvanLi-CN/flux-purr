#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

mod protocol;

#[cfg(target_arch = "xtensa")]
mod device {
    use super::protocol::{self, Command};
    use embassy_time::{Duration, Timer};
    use embedded_hal_async::spi::SpiBus;
    use embedded_io_async::{Read, Write};
    use esp_hal::{
        Async, Blocking,
        analog::adc::{Adc, AdcCalBasic, AdcConfig, AdcPin, Attenuation},
        gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
        i2c::master::{Config as I2cConfig, I2c},
        spi::{
            Mode as SpiMode,
            master::{Config as SpiConfig, Spi},
        },
        time::Rate,
        usb_serial_jtag::UsbSerialJtag,
    };
    use heapless::String;

    const LINE_MAX: usize = 512;

    type AdcDriver = Adc<'static, esp_hal::peripherals::ADC1<'static>, Blocking>;
    type AdcPin1 = AdcPin<
        esp_hal::peripherals::GPIO1<'static>,
        esp_hal::peripherals::ADC1<'static>,
        AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
    >;
    type AdcPin2 = AdcPin<
        esp_hal::peripherals::GPIO2<'static>,
        esp_hal::peripherals::ADC1<'static>,
        AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
    >;
    type I2cBus = I2c<'static, Async>;
    type DisplayBus = Spi<'static, Async>;

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
                },
            )
        }
    }

    pub struct Outputs {
        heater: Output<'static>,
        fan: Output<'static>,
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

    impl Outputs {
        fn new(tokens: PeripheralTokens) -> (Self, Inputs, Measurements) {
            let input_config = InputConfig::default().with_pull(Pull::Up);
            let inputs = Inputs {
                center: Input::new(tokens.gpio0, input_config),
                right: Input::new(tokens.gpio16, input_config),
                down: Input::new(tokens.gpio17, input_config),
                left: Input::new(tokens.gpio18, input_config),
                up: Input::new(tokens.gpio21, input_config),
            };
            let mut adc_config = AdcConfig::new();
            let vin = adc_config
                .enable_pin_with_cal::<_, AdcCalBasic<_>>(tokens.gpio1, Attenuation::_11dB);
            let rtd = adc_config
                .enable_pin_with_cal::<_, AdcCalBasic<_>>(tokens.gpio2, Attenuation::_11dB);
            let adc = Adc::new(tokens.adc1, adc_config);
            let i2c = I2c::new(
                tokens.i2c0,
                I2cConfig::default().with_frequency(Rate::from_khz(400)),
            )
            .expect("failed to create bring-up I2C")
            .with_sda(tokens.gpio8)
            .with_scl(tokens.gpio9)
            .into_async();
            let display = Spi::new(
                tokens.spi2,
                SpiConfig::default()
                    .with_frequency(Rate::from_mhz(40))
                    .with_mode(SpiMode::_0),
            )
            .expect("failed to create bring-up display SPI")
            .with_sck(tokens.gpio12)
            .with_mosi(tokens.gpio11)
            .into_async();
            let outputs = Self {
                heater: Output::new(tokens.gpio47, Level::Low, OutputConfig::default()),
                fan: Output::new(tokens.gpio35, Level::Low, OutputConfig::default()),
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
            (outputs, inputs, Measurements { adc, vin, rtd, i2c })
        }

        fn safe(&mut self) {
            self.heater.set_low();
            self.fan.set_low();
            self.buzzer.set_low();
            self.backlight.set_high();
            self.display_reset.set_high();
            self.display_cs.set_high();
            self.display_dc.set_low();
            self.red.set_high();
            self.green.set_high();
            self.blue.set_high();
        }

        fn rgb(&mut self, red: bool, green: bool, blue: bool) {
            self.red
                .set_level(if red { Level::Low } else { Level::High });
            self.green
                .set_level(if green { Level::Low } else { Level::High });
            self.blue
                .set_level(if blue { Level::Low } else { Level::High });
        }

        async fn display_preview(&mut self, color: [u8; 2]) -> bool {
            self.display_reset.set_low();
            Timer::after(Duration::from_millis(5)).await;
            self.display_reset.set_high();
            Timer::after(Duration::from_millis(120)).await;
            if !self.command(0x11, &[]).await {
                return false;
            }
            Timer::after(Duration::from_millis(20)).await;
            for (command, data) in [
                (0x3a, &[0x55][..]),
                (0x36, &[0x48][..]),
                (0x2a, &[0x00, 0x00, 0x00, 0x9f][..]),
                (0x2b, &[0x00, 0x00, 0x00, 0x31][..]),
                (0x2c, &[][..]),
            ] {
                if !self.command(command, data).await {
                    return false;
                }
            }
            self.display_dc.set_high();
            self.display_cs.set_low();
            let mut pixels = [0u8; 128];
            for chunk in pixels.chunks_exact_mut(2) {
                chunk.copy_from_slice(&color);
            }
            for _ in 0..125 {
                if SpiBus::write(&mut self.display, &pixels).await.is_err() {
                    self.display_cs.set_high();
                    return false;
                }
            }
            self.display_cs.set_high();
            true
        }

        async fn command(&mut self, command: u8, data: &[u8]) -> bool {
            self.display_cs.set_low();
            self.display_dc.set_low();
            let command_ok = SpiBus::write(&mut self.display, &[command]).await.is_ok();
            if !data.is_empty() {
                self.display_dc.set_high();
                if !command_ok || SpiBus::write(&mut self.display, data).await.is_err() {
                    self.display_cs.set_high();
                    return false;
                }
            }
            self.display_cs.set_high();
            command_ok
        }
    }

    pub async fn run() -> ! {
        let peripherals = esp_hal::init(esp_hal::Config::default());
        let (usb_device, tokens) = PeripheralTokens::split(peripherals);
        let mut usb = UsbSerialJtag::new(usb_device).into_async();
        let mut hello = String::<1024>::new();
        let _ = protocol::write_identity(&mut hello);
        let _ = usb.write_all(hello.as_bytes()).await;
        let (mut outputs, mut inputs, mut measurements) = Outputs::new(tokens);
        outputs.safe();
        let mut line = [0u8; LINE_MAX];
        let mut length = 0usize;
        loop {
            let mut byte = [0u8; 1];
            if usb.read(&mut byte).await.is_err() {
                outputs.safe();
                continue;
            }
            if byte[0] == b'\n' {
                if length > 0 {
                    handle_line(
                        &mut outputs,
                        &mut inputs,
                        &mut measurements,
                        &mut usb,
                        &line[..length],
                    )
                    .await;
                }
                length = 0;
            } else if length < line.len() {
                line[length] = byte[0];
                length += 1;
            } else {
                length = 0;
                outputs.safe();
            }
        }
    }

    async fn handle_line(
        outputs: &mut Outputs,
        inputs: &mut Inputs,
        measurements: &mut Measurements,
        usb: &mut UsbSerialJtag<'static, esp_hal::Async>,
        line: &[u8],
    ) {
        outputs.safe();
        let Some((request, _)) = protocol::parse_request(line) else {
            return;
        };
        let Some(command) = request.command() else {
            return;
        };
        let (ok, detail) = match command {
            Command::PreviewDisplay | Command::PreviewFrontpanel => {
                outputs.backlight.set_low();
                let ok = outputs
                    .display_preview(if matches!(command, Command::PreviewFrontpanel) {
                        [0x07, 0xe0]
                    } else {
                        [0xf8, 0x00]
                    })
                    .await;
                if ok {
                    outputs.rgb(false, true, true);
                    (true, "display_preview_ready")
                } else {
                    outputs.safe();
                    (false, "display_preview_failed")
                }
            }
            Command::PreviewStatusLight => {
                outputs.rgb(false, true, false);
                (true, "status_light_preview_ready")
            }
            Command::TestButtons => {
                let _states = [
                    inputs.center.is_low(),
                    inputs.right.is_low(),
                    inputs.down.is_low(),
                    inputs.left.is_low(),
                    inputs.up.is_low(),
                ];
                (true, "buttons_read_only_ready")
            }
            Command::TestAdc => {
                let vin_ok = measurements.adc.read_oneshot(&mut measurements.vin).is_ok();
                let rtd_ok = measurements.adc.read_oneshot(&mut measurements.rtd).is_ok();
                (
                    vin_ok && rtd_ok,
                    if vin_ok && rtd_ok {
                        "adc_read_only_ready"
                    } else {
                        "adc_read_failed"
                    },
                )
            }
            Command::TestI2c => {
                let address = request.address.unwrap_or(0x22);
                let register =
                    request
                        .register
                        .unwrap_or(if address == 0x22 { 0x09 } else { 0x00 });
                let mut value = [0u8; 1];
                let ok = measurements
                    .i2c
                    .write_read_async(address, &[register], &mut value)
                    .await
                    .is_ok();
                (
                    ok,
                    if ok {
                        "i2c_identification_read_only_ready"
                    } else {
                        "i2c_identification_read_failed"
                    },
                )
            }
            Command::TestRgb => {
                outputs.rgb(true, false, false);
                Timer::after(Duration::from_millis(100)).await;
                outputs.rgb(false, true, false);
                Timer::after(Duration::from_millis(100)).await;
                outputs.rgb(false, false, true);
                Timer::after(Duration::from_millis(100)).await;
                outputs.rgb(false, false, false);
                (true, "rgb_ready")
            }
            Command::TestBuzzer => {
                outputs.buzzer.set_high();
                Timer::after(Duration::from_millis(20)).await;
                outputs.buzzer.set_low();
                (true, "buzzer_ready")
            }
            Command::TestFan => {
                outputs.fan.set_high();
                Timer::after(Duration::from_millis(500)).await;
                outputs.fan.set_low();
                (true, "fan_ready")
            }
            Command::Exit => {
                outputs.safe();
                (true, "safe_exit")
            }
        };
        outputs.heater.set_low();
        if !ok {
            outputs.safe();
        }
        let mut response = String::<512>::new();
        let _ = protocol::write_response(
            &mut response,
            request.request_id.as_str(),
            command,
            ok,
            detail,
        );
        let _ = usb.write_all(response.as_bytes()).await;
        if matches!(command, Command::Exit) {
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
