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
status: active
related_specs:
  - docs/specs/buzzer-cue-arbitration/SPEC.md
  - docs/specs/q2aw6-heater-pid-frontpanel-runtime/SPEC.md
---

# ESP32-S3 MCPWM carrier readback

## Context

Multi-tone buzzer cues can sound as if every step has one pitch even when the
cue controller and MCPWM configuration trace contain different frequencies.
The timer configuration register is not sufficient evidence that the GPIO pad
emitted the corresponding carrier.

## Symptoms

- Cue traces contain the expected frequency sequence, but every audible step
  has the startup pitch.
- Rapid replay can make the apparent pitch change once and then remain fixed.
- MCPWM `prescaler` and `period` readback predicts the requested frequencies,
  while an external listener cannot distinguish them.
- Reordering duty, timer stop, counter reset, and timer start does not change
  the symptom.

## Root cause model

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

- Keep one representable MCPWM timer prescaler for all production buzzer
  frequencies.
- Derive the period from the requested frequency and round the timer counts to
  the nearest integer.
- For a different-frequency step, set duty to zero, stop the timer, reset its
  counter, apply the new period, restart the timer, and restore duty.
- Preserve the timer through same-frequency duty-zero rests and replays.
- In debug builds, report requested frequency, timer-derived frequency, PCNT
  pad frequency, edge count, observation window, duty, and generation together.
- Keep PCNT initialization and its USB/devd fields behind the diagnostic
  feature; production builds retain only the corrected PWM path.

## Validation

1. Unit-test that every production frequency fits the fixed-prescaler period
   range and that rounding error stays within the accepted bound.
2. Unit-test that a different frequency orders hardware actions as silence,
   timer stop, retune, and duty restore.
3. Run a multi-tone production cue through the ordinary arbiter and real-time
   GPIO owner on an authorized device.
4. Require PCNT pad observations to follow every requested tone rather than
   accepting timer configuration readback alone.
5. Confirm the same sequence acoustically, then explicitly stop repeated debug
   playback.

## Guardrails and reuse notes

- Do not infer a buzzer or acoustic fault until the GPIO carrier has been
  measured independently of the PWM configuration registers.
- Do not add a raw-frequency endpoint to obtain diagnostic coverage. Submit
  production cue IDs through the production arbiter.
- A pad-level PCNT result validates the digital waveform, not sound pressure,
  resonance, polarity, or analog drive current.
- Keep the observation bounded and read-only. It must not become a second owner
  of the output or alter production timing.

## References

- [Buzzer cue arbitration implementation](../../specs/buzzer-cue-arbitration/IMPLEMENTATION.md)
- [Single-output buzzer cue arbitration ADR](../../adr/0006-single-output-buzzer-cue-arbitration.md)
