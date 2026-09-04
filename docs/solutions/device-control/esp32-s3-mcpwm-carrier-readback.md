---
title: ESP32-S3 MCPWM carrier readback
module: device-control
problem_type: diagnosis
component: firmware-pwm
tags:
  - esp32-s3
  - mcpwm
  - pcnt
  - buzzer
  - gpio
  - cue-arbitration
  - hardware-in-the-loop
status: active
related_specs:
  - docs/specs/buzzer-cue-arbitration/SPEC.md
  - docs/specs/heater-pid-frontpanel-runtime/SPEC.md
symptoms: Unexpected cue changes, compressed rests, or a multi-tone cue with one apparent pitch.
root_cause: Multiple cue owners or timer configuration that does not change the GPIO carrier.
resolution_type: Single-owner cue arbitration with pad-level carrier validation.
---

# ESP32-S3 MCPWM carrier readback

## Context

Multi-tone buzzer cues can sound as if every step has one pitch even when the
cue controller and MCPWM configuration trace contain different frequencies.
They can also change cue or compress a rest when multiple runtime paths own one
GPIO output. These symptoms must be separated before changing hardware timing:
the timer configuration register is not sufficient evidence that the GPIO pad
emitted the corresponding carrier, and a valid carrier does not prove that cue
selection and cadence have one owner.

## Symptoms

- Cue traces contain the expected frequency sequence, but every audible step
  has the startup pitch.
- Rapid replay can make the apparent pitch change once and then remain fixed.
- MCPWM `prescaler` and `period` readback predicts the requested frequencies,
  while an external listener cannot distinguish them.
- Reordering duty, timer stop, counter reset, and timer start does not change
  the symptom.
- A cue's rests are occasionally shortened or merged, especially while USB,
  display, or control-loop work is active.
- An unrelated feedback cue appears to replace a safety cue or to change the
  current cue before its production pattern has completed.

## Root cause

### Separate cue ownership from carrier generation

One GPIO output can only emit one cue. Direct calls from independent feedback,
thermal, startup, and control-plane paths create a software ownership race:
the last writer can replace a current cue and any shared loop can delay or skip
a tone/rest step. This failure remains possible even when every requested
frequency reaches the pad correctly.

Route all producers through one arbiter and let one real-time task own cue
selection, step deadlines, Timer2, and GPIO48 duty writes. The task advances at
most one pending step per wake-up and starts that step's duration when it is
actually emitted. This prevents a delayed wake-up from swallowing a short rest.

After that ownership boundary is in place, treat the physical carrier as a
separate hypothesis. A configuration trace proves that software wrote a
register; it does not prove that GPIO48 changed waveform.

### Dynamic prescaler did not prove a changed pad waveform

Flux Purr originally kept one period and changed Timer2's prescaler for each
pitch. On the affected ESP32-S3 path, `CFG0` reported each new prescaler while
GPIO48 continued to emit the timer's startup carrier. A configuration readback
therefore produced a false positive: it proved a register write, not the pad
waveform.

The decisive diagnostic is a second hardware peripheral observing the output
pad. A feature-gated PCNT channel can count GPIO48 rising edges without changing
the production cue, arbitration, or PWM path. For an audible step, the observed
frequency is:

`rising_edges * 1000 / observation_window_ms`

Associate the result with the preceding output step, because the transition at
the end of that step closes its observation window.

## Resolution

- Submit every cue source to `BuzzerArbiter`; do not grant application code a
  direct PWM, timer, or duty-write path.
- Run the arbiter and playback engine in one dedicated real-time task. Preserve
  every production tone/rest step and restart cadence only through the normal
  production request path.
- Keep one representable MCPWM timer prescaler for all production buzzer
  frequencies.
- Derive the period from the requested frequency and round the timer counts to
  the nearest integer.
- For a different-frequency step, set duty to zero, stop the timer, reset its
  counter, apply the new period, restart the timer, and restore duty.
- Preserve the timer through same-frequency duty-zero rests and replays.
- In `buzzer-observe` builds, report requested frequency, timer-derived frequency, PCNT
  pad frequency, edge count, observation window, duty, and generation together.
- Keep PCNT initialization and its USB/devd fields behind the diagnostic
  feature; the standard build retains the formal buzzer test path, while
  production images without `buzzer-test` retain only the corrected PWM path.

## Validation

1. Unit-test arbiter priority, pending-feedback replacement, safety-state
   suppression, and delayed cue starts separately from PWM timing.
2. Unit-test that every production frequency fits the fixed-prescaler period
   range, rounding error stays within the accepted bound, and a delayed tick
   cannot skip a tone/rest step.
3. Unit-test that a different frequency orders hardware actions as silence,
   timer stop, retune, and duty restore; assert that same-frequency rests reuse
   the carrier.
4. Build the observer firmware from the regular runtime with the optional
   observation feature enabled:

   ```bash
   cargo +esp build --manifest-path firmware/Cargo.toml \
     --target xtensa-esp32s3-none-elf \
     --target-dir firmware/target/buzzer-observe \
     --release --features buzzer-test,buzzer-observe
   ```

5. On an owner-authorized exact device and only after a dry-run firmware
   verification, submit a production cue through `flux-purr -> flux-purr-devd`:

   ```bash
   cargo run --manifest-path tools/flux-purr-devd/Cargo.toml --bin flux-purr -- \
     buzzer test --devd http://127.0.0.1:<leased-devd-port> \
     --device <device-id> --cue active-cooling-on --json
   ```

6. Require PCNT pad observations to follow every requested tone rather than
   accepting timer configuration readback alone. Confirm the same sequence
   acoustically, then send the explicit `--stop` request before releasing the
   device lease.

## Guardrails and reuse notes

- Do not infer a buzzer or acoustic fault until the GPIO carrier has been
  measured independently of the PWM configuration registers.
- If PCNT follows each requested tone but the cadence is still irregular, stop
  changing PWM registers and inspect cue ownership, arbiter decisions, and the
  real-time task's step scheduling instead.
- Do not add a raw-frequency endpoint to obtain diagnostic coverage. Submit
  production cue IDs through the production arbiter.
- Keep the diagnostic feature disabled in ordinary firmware. Its feature build
  must reuse the same startup, safety interlocks, arbiter, Timer2, and GPIO48
  path as production; a recovery-only playback path is not valid evidence.
- A pad-level PCNT result validates the digital waveform, not sound pressure,
  resonance, polarity, or analog drive current.
- Keep the observation bounded and read-only. It must not become a second owner
  of the output or alter production timing.
- Repeated playback must require an explicit request and an explicit stop. Do
  not leave a continuous alarm or feedback test active after collecting the
  trace.

## References

- [Buzzer cue arbitration implementation](../../specs/buzzer-cue-arbitration/IMPLEMENTATION.md)
- [Single-output buzzer cue arbitration ADR](../../adr/0006-single-output-buzzer-cue-arbitration.md)
- [Firmware buzzer contract](../../../firmware/README.md)
