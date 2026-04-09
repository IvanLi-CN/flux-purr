#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

#[cfg(target_arch = "xtensa")]
use defmt::info;
#[cfg(target_arch = "xtensa")]
use esp_backtrace as _;
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
use esp_println as _;
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{
    FAN_PHASE_DURATION_SECS, FAN_PWM_FREQUENCY_HZ, FAN_STOP_SAFE_PWM_PERMILLE, FanCommand,
    FanCycleController, FanPhase, board::s3_frontpanel, pwm_percent_from_permille,
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
fn phase_label(phase: FanPhase) -> &'static str {
    match phase {
        FanPhase::High => "high",
        FanPhase::Low => "low",
        FanPhase::Mid => "mid",
        FanPhase::Stop => "stop",
    }
}

#[cfg(target_arch = "xtensa")]
fn configure_timer_with_fallback<T>(timer: &mut T) -> Result<u8, timer::Error>
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
        .map(|_| 10)
        .or_else(|_| {
            timer.configure(timer::config::Config {
                duty: Duty::Duty8Bit,
                clock_source: LSClockSource::APBClk,
                frequency: freq,
            })?;
            Ok(8)
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
fn log_command(command: FanCommand, uptime_secs: u32) {
    info!(
        "fan phase={=str} enabled={=bool} pwm_permille={=u16} uptime_s={=u32}",
        phase_label(command.phase),
        command.enabled,
        command.pwm_permille,
        uptime_secs,
    );
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
    let timer_bits = configure_timer_with_fallback(&mut timer0)
        .expect("failed to configure LEDC timer for fan PWM");

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
    info!(
        "boot fan_en_gpio={=u8} fan_pwm_gpio={=u8} fan_tach_gpio={=u8} pwm_hz={=u32} timer_bits={=u8}",
        s3_frontpanel::PIN_FAN_EN,
        s3_frontpanel::PIN_FAN_PWM,
        s3_frontpanel::PIN_FAN_TACH,
        FAN_PWM_FREQUENCY_HZ,
        timer_bits,
    );
    apply_command(&mut fan_enable, &fan_pwm, initial);
    log_command(initial, uptime_secs);

    loop {
        let phase_started = Instant::now();
        while phase_started.elapsed() < Duration::from_secs(FAN_PHASE_DURATION_SECS as u64) {}
        uptime_secs = uptime_secs.saturating_add(FAN_PHASE_DURATION_SECS);
        let command = controller.command_at(uptime_secs);
        apply_command(&mut fan_enable, &fan_pwm, command);
        log_command(command, uptime_secs);
    }
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {
    println!(
        "esp32s3-fan-cycle is a host stub; build with --target xtensa-esp32s3-none-elf --features esp32s3"
    );
}
