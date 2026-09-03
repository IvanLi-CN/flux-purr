# Firmware-owned thermal tuning core

## Status

Accepted

## Context

Formal multi-target thermal tuning is currently orchestrated by the native
`flux-purr thermal tune` command. The Device already owns heater safety,
thermal control, PPS contract negotiation, and bounded long-running thermal
workflows, while the Web Control Console has no equivalent tuning workflow.

The product needs a single production algorithm across firmware, CLI, and Web
without making a run depend on a browser, daemon, or CLI remaining connected.
It must support only the explicit PPS `pps3a` and `pps5a` power classes. The
existing host-driven CLI workflow is a long-lived reference implementation: it
provides a live fallback, an independent cross-check, and an environment for
improving tuning behavior before changing firmware.

`pps3a` denotes the established 3A-class PPS tier, including the existing
65W / `20V @ 3250mA` source capability. It is not an exact `3000mA` current
ceiling. `pps5a` denotes the 5A-class tier.

The native `flux-purr` CLI is the established host-side owner for detailed run
files, report generation, and reference analysis in a CLI-initiated workflow.
The Control Console is a separate host surface and must retain its own
browser-local archive and report path. Web and CLI cannot communicate, start
one another, or relay tuning data to one another. `devd` is a Device
communication, protocol-adaptation, hardware-memory, and hardware-reuse
service; it must not become a second tuning orchestrator.

## Decision

- A new deterministic, allocation-free Rust thermal-tuning core will be
  introduced as a `no_std` workspace library. Firmware invokes it as the sole
  authority for target scheduling, candidate generation, scoring, acceptance,
  and terminal disposition. Its decision values and candidate hashes use
  canonical fixed-point representation so firmware, native Rust, and Wasm can
  replay the same result exactly; this does not require replacing the existing
  floating-point heater PID execution.
- Every Thermal Tuning Run carries an explicit `pps3a` or `pps5a` power-class
  selection. The firmware rejects an unavailable selected class and never
  resolves `auto` or downgrades to the other class. Existing ordinary-runtime
  `auto|65w|100w` compatibility remains outside the tuning contract.
- A started run is Device-owned. It continues when the Control Console, CLI,
  or `devd` disconnects. Firmware alone processes cancellation and immediately
  disarms on PPS-contract loss, measurement invalidity, or any thermal safety
  condition.
- A Device reset, power loss, or failed startup immediately disarms and writes
  the recoverable terminal disposition `interrupted_reset`. Firmware never
  resumes such a run, reuses its in-progress candidate, or permits that
  candidate to be previewed or saved.
- Firmware persists only a compact two-phase Tuning Journal: one start marker
  and one terminal or recovery summary for the latest run. It never persists
  raw telemetry or a promotable candidate, limiting EEPROM/flash writes while
  still making an interrupted reset observable after reboot.
- The same production core builds natively for CLI consumers and to WebAssembly
  for the Control Console. Those consumers may render, replay, and verify a
  Device result, but they do not decide or advance a live run.
- Firmware requests and maintains the PPS contract through its existing PD
  controller. Product tuning only consumes Device-observed PPS capability; it
  exposes no IsolaPurr or host source-control operation in the Control Console
  or `devd`. Bench-source preparation remains a developer/HIL fixture concern
  outside the product tuning contract.
- Product tuning evidence and acceptance use only Device-local temperature,
  VIN, PPS-contract, and control-output observations. Neither the new CLI nor
  the Control Console queries another device for VBUS current, voltage, or
  power. Contractual current remains a firmware safety limit, not a measured
  current. Historical bench-source diagnostics may remain within the separate
  reference implementation, outside `thermal-tuning-v2`.
- `thermal_tuning_run` is a separate first-class Device capability. It shares
  a Maintenance Run Arbiter with manual heating and automatic calibration; a
  start request is rejected with the active owner when the Device is busy, and
  never implicitly stops, cancels, or resumes another operation.
- Firmware declares `thermal_tuning_run_v1` before a Control Console may start
  or observe a tuning run. A device without that capability is explicitly
  incompatible with the new Control Console workflow; the Control Console does
  not invoke host-reference behavior as a fallback.
- Firmware accepts a tuning start only when its active thermal model and
  heater-curve coverage are valid, the explicitly selected PPS class is
  available, and the Maintenance Run Arbiter is idle. It does not automatically
  start automatic thermal-model calibration to satisfy these prerequisites.
- A review-complete candidate is identified by its candidate ID, selected power
  class, and content hash. Preview applies that exact profile only in RAM and
  verifies Device readback without a further heating cycle. Saving to EEPROM
  requires a second simple confirmation and is accepted only for the unchanged,
  successfully previewed candidate in its matching bank.
- In a CLI-initiated workflow, the CLI Tuning Host Runner owns detailed
  telemetry recording, report generation, and reference comparison. In a
  Control Console-initiated workflow, browser-local Wasm code owns the
  detailed telemetry archive, replay, and report export. The Control Console
  records automatically in browser-local persistent storage, without a
  file-selection, upload, or credential step, and offers explicit report
  export after the run. These two host surfaces do not communicate.
- `devd` remains a bounded transport and hardware service. It does not own
  tuning policy, run records, report generation, CLI process lifecycle, or a
  Web-to-CLI relay.
- The current host-driven algorithm remains available to the CLI as an explicit
  `host-reference` engine. It uses the common transport, safety, input, and
  report contracts, but retains an independent optimizer so it can compare
  decision records with the firmware engine, serve as a live fallback, and
  evaluate algorithm changes before a firmware update. It is not exposed as a
  normal Web workflow and may be removed only with explicit Operator approval.
- A CLI reference comparison reports `equivalent`, `divergent`,
  `inconclusive`, or `not_run`. It is diagnostic evidence for algorithm work
  and HIL/release validation, not a runtime promotion gate; a Device-authoritative
  review-complete candidate remains eligible for preview and save.
- Firmware persists a compact, recoverable decision journal and terminal
  summary. Detailed telemetry is streamed to a host recorder; a run that lacks
  a complete host archive is `review-incomplete` and cannot preview or save its
  candidate profile.
- `thermal_tuning_run` exposes detailed telemetry as a bounded, paged stream
  with monotonically increasing sequence numbers. A client records the stream
  locally; buffer coverage or any detected missing sequence is returned as
  `trace_gap` and makes the run `review-incomplete`, rather than silently
  omitting evidence.
- CLI and Control Console export the same `thermal-tuning-v2` audit bundle:
  `index.html`, `run.bundle.json`, `samples.ndjson`,
  `thermal-profile.candidate.json`, and `decision-ledger.ndjson`. The Control
  Console exports those files as a browser-generated ZIP. Historical
  `thermal-profile.accepted.json` input remains import-compatible only.

## Consequences

Firmware needs a bounded tuning-run projection, persistent terminal summary,
and transport capability so every supported connection can start, observe, and
later recover a run. The core needs deterministic integer or otherwise
cross-target-stable arithmetic, explicit time inputs, and strict fixed-capacity
data structures suitable for the MCU. The exclusive maintenance arbiter must
be surfaced consistently in every transport so clients can explain an occupied
Device without attempting a conflicting write.

CLI and Web code move from owning live tuning decisions to consuming the
Device-owned run protocol. CLI remains the host runner for CLI workflows,
additionally retains the explicit reference engine, and produces structured
per-decision comparison evidence. The Control Console independently persists
and exports its own browser-local evidence through the Wasm build. Browser
storage failure, browser closure, or a telemetry gap leaves the Device run
safe but makes its candidate `review-incomplete`. The
reference engine intentionally carries continuing maintenance cost, but
protects development and hardware validation from a single new implementation
failure and allows iteration before consuming firmware-update or device-test
budget.

## Alternatives considered

- Keeping the host CLI as the production authority would leave direct Web
  Serial and LAN operation dependent on host orchestration and prevent a run
  from completing after the host disconnects.
- Moving the production algorithm into WebAssembly would make browser lifetime
  and transport reliability part of heater control.
- Reusing the exact production optimizer for the reference engine would verify
  transport integration but would not provide an independent algorithmic
  cross-check.
