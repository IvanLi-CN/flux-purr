#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

#[cfg(target_arch = "xtensa")]
use core::panic::PanicInfo;
#[cfg(target_arch = "xtensa")]
use esp_hal::{
    clock::CpuClock,
    gpio::{DriveMode, Level, Output, OutputConfig},
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed, channel,
        channel::{ChannelIFace, config::Config as ChannelConfig},
        timer,
        timer::{LSClockSource, TimerIFace, config::Duty},
    },
    main,
    time::{Duration, Instant, Rate},
};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{
    FAN_PHASE_DURATION_SECS, FAN_PWM_FREQUENCY_HZ, FAN_STOP_SAFE_PWM_PERMILLE, FanCommand,
    FanCycleController, board::s3_frontpanel, pwm_percent_from_permille,
};
#[cfg(target_arch = "xtensa")]
esp_bootloader_esp_idf::esp_app_desc!();
#[cfg(target_arch = "xtensa")]
const _: [(); s3_frontpanel::PIN_FAN_EN as usize] = [(); 35];
#[cfg(target_arch = "xtensa")]
const _: [(); s3_frontpanel::PIN_FAN_PWM as usize] = [(); 36];
#[cfg(target_arch = "xtensa")]
const _: [(); s3_frontpanel::PIN_FAN_TACH as usize] = [(); 34];

#[cfg(target_arch = "xtensa")]
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    esp_hal::system::software_reset()
}

#[cfg(target_arch = "xtensa")]
fn configure_timer_with_fallback<T>(timer: &mut T) -> Result<(), timer::Error>
where
    T: TimerIFace<LowSpeed>,
{
    let freq = Rate::from_hz(FAN_PWM_FREQUENCY_HZ);

    timer
        .configure(timer::config::Config {
            duty: Duty::Duty10Bit,
            clock_source: LSClockSource::APBClk,
            frequency: freq,
        })
        .or_else(|_| {
            timer.configure(timer::config::Config {
                duty: Duty::Duty8Bit,
                clock_source: LSClockSource::APBClk,
                frequency: freq,
            })
        })
}

#[cfg(target_arch = "xtensa")]
fn apply_command<'a, C>(fan_enable: &mut Output<'_>, channel: &C, command: FanCommand)
where
    C: ChannelIFace<'a, LowSpeed>,
{
    let safe_percent = pwm_percent_from_permille(FAN_STOP_SAFE_PWM_PERMILLE);
    if command.enabled {
        let duty_percent = pwm_percent_from_permille(command.pwm_permille);
        assert!(
            channel.set_duty(duty_percent).is_ok(),
            "failed to update LEDC duty while enabling fan"
        );
        fan_enable.set_high();
    } else {
        fan_enable.set_low();
        assert!(
            channel.set_duty(safe_percent).is_ok(),
            "failed to update LEDC duty while stopping fan"
        );
    }
}

#[cfg(target_arch = "xtensa")]
#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let mut fan_enable = Output::new(peripherals.GPIO35, Level::Low, OutputConfig::default());

    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    let mut timer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    assert!(
        configure_timer_with_fallback(&mut timer0).is_ok(),
        "failed to configure LEDC timer for fan PWM"
    );

    let mut fan_pwm = ledc.channel(channel::Number::Channel0, peripherals.GPIO36);
    let initial = FanCycleController::new().command_at(0);
    assert!(
        fan_pwm
            .configure(ChannelConfig {
                timer: &timer0,
                duty_pct: pwm_percent_from_permille(initial.pwm_permille),
                drive_mode: DriveMode::PushPull,
            })
            .is_ok(),
        "failed to configure LEDC fan PWM channel"
    );

    let mut controller = FanCycleController::new();
    let mut uptime_secs = 0_u32;
    apply_command(&mut fan_enable, &fan_pwm, initial);

    loop {
        let phase_started = Instant::now();
        while phase_started.elapsed() < Duration::from_secs(FAN_PHASE_DURATION_SECS as u64) {}
        uptime_secs = uptime_secs.saturating_add(FAN_PHASE_DURATION_SECS);
        let command = controller.command_at(uptime_secs);
        apply_command(&mut fan_enable, &fan_pwm, command);
    }
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {
    println!(
        "esp32s3-fan-cycle is a host stub; build with --target xtensa-esp32s3-none-elf --features esp32s3"
    );
}
