# Single-output buzzer cue arbitration

## Status

Accepted

## Context

GPIO48 drives one passive buzzer through one PWM output. The previous playback
API allowed every caller to replace the active cue directly, so independent
front-panel, runtime-control, and thermal-attention requests could cut across
one another. The carrier-preservation fix remains necessary, but it cannot
decide which cue owns the output.

## Decision

`BuzzerArbiter` is the only owner that selects playback for the single buzzer
output. A dedicated Embassy task in a priority-2 software-interrupt executor
owns the arbiter, lower-level controller, and every GPIO48 PWM write; runtime
callers submit bounded cue requests. This isolates cue-step deadlines from
cooperative display, USB, and control work. There is no mixing and no direct
caller preemption.

- `ProtectionAlarm` is the highest-priority cue. Its established four-step
  rhythm (`2300Hz`, rest, `2300Hz`, rest) remains a non-looping one-shot, played immediately on
  thermal-runaway entry and then at the existing one-second cadence. It immediately preempts
  lower-priority playback and clears retained feedback.
- `AttentionReminder` is lower than `ProtectionAlarm` and higher than ordinary
  feedback. It waits for an already-started feedback cue to finish, replaces
  any retained feedback, and keeps its existing ten-second cadence after an
  unacknowledged runaway has cleared.
- `FeedbackCue` covers accepted interaction and runtime-operation feedback. It
  never interrupts an active cue. The arbiter retains at most one pending
  feedback cue: repeated `ui_input` requests coalesce, while the latest
  specialized state or rejection cue replaces the retained feedback.
- An Audible Safety State suppresses all ordinary feedback. Entering, leaving,
  or acknowledging that state clears retained feedback; no suppressed or stale
  feedback is replayed afterward.
- The lower-level controller still owns cue steps, silence, and PWM carrier
  preservation. Its real-time task wakes at every cue-step deadline; if it is
  late, it emits the next unobserved step before advancing again, so a short
  silence cannot be skipped. Timer2 keeps one prescaler and selects pitch with
  its period. A different-frequency step first silences GPIO48, stops Timer2,
  resets its counter, applies the new period, starts the timer, and only then
  restores duty. Same-frequency silence keeps Timer2 running.
  Arbiter decision logs identify the request source, cue, and selected
  disposition without creating a new product API or persistent state.
- The default `buzzer-test` firmware feature exposes a native-USB/devd
  test session after declaring the `buzzer_test` identity capability. It can
  submit production cue IDs or fixed arbitration scenarios, is rejected while
  real thermal safety is active, and cannot set PWM parameters, persist state,
  or use LAN. It reuses the normal arbiter, cue patterns, GPIO48 output path,
  and protection/reminder cadence; `protection_alarm --repeat` is an explicit
  controlled test of the same one-second safety cadence. The feature adds only
  this test session to ordinary runtime initialization and never selects a
  recovery-only audio path. An optional `buzzer-observe` feature reads MCPWM
  timer2's prescaler and period and uses a feature-only PCNT input to count
  rising edges from the GPIO48 pad after the ordinary real-time task applies
  each cue step.
  This distinguishes timer configuration from the emitted digital carrier
  without claiming an acoustic measurement or adding a raw-PWM control surface.

## Consequences

The audible behavior becomes deterministic even when several command sources
are processed in one main-loop pass or that loop is delayed. An accepted
interaction may now have its feedback coalesced or suppressed by a safety
state, so callers must treat a feedback request as subject to arbitration
rather than as immediate playback. Regression coverage belongs at the arbiter
seam, the step-deadline path, and the runtime paths that schedule thermal
attention.

## Alternatives considered

- Unconditional last-request-wins preemption produces the reported mid-cue
  tone changes.
- A general FIFO preserves stale input feedback and can produce a delayed run
  of no-longer-meaningful sounds.
- Mixing is incompatible with the single physical PWM output and would make
  the safety signal harder to distinguish.
