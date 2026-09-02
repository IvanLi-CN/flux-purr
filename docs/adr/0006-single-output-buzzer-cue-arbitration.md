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
output. It sends one selected cue at a time to the lower-level controller;
there is no mixing and no direct caller preemption.

- `ProtectionAlarm` is the highest-priority cue. Its existing internal tone
  pattern is a non-looping one-shot, played immediately on thermal-runaway
  entry and then at the existing one-second cadence. It immediately preempts
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
  preservation. Arbiter decision logs identify the request source, cue, and
  selected disposition without creating a new product API or persistent state.
- A non-default `buzzer-debug` firmware feature may expose a native-USB/devd
  diagnostic after declaring the `buzzer_debug` identity capability. It can
  submit only fixed feedback cues or fixed arbitration scenarios as a
  `DeveloperDebug` source, is rejected while thermal safety is active, and
  cannot set PWM parameters, emit safety cues, persist state, or use LAN.

## Consequences

The audible behavior becomes deterministic even when several command sources
are processed in one main-loop pass. An accepted interaction may now have its
feedback coalesced or suppressed by a safety state, so callers must treat a
feedback request as subject to arbitration rather than as immediate playback.
Regression coverage belongs at the arbiter seam and at the runtime paths that
schedule thermal attention.

## Alternatives considered

- Unconditional last-request-wins preemption produces the reported mid-cue
  tone changes.
- A general FIFO preserves stale input feedback and can produce a delayed run
  of no-longer-meaningful sounds.
- Mixing is incompatible with the single physical PWM output and would make
  the safety signal harder to distinguish.
