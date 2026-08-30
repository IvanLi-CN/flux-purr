# Thermal Tuning CLI Contract

## Firmware Engine

The product CLI path is a host recorder and report writer for the Device-owned engine:

```text
flux-purr thermal tune --engine firmware --power-class pps3a --output-dir <directory>
```

`--engine firmware` is the normal product mode. It discovers or consumes the existing chosen
Flux Purr transport, calls only `thermal_tuning_run_v1`, continuously pages/atomically records
trace events, sends `ack_trace` and `seal_review`, and writes a `thermal-tuning-v2` bundle.
It may issue preview/save only after the Device reports a review-complete candidate and the
CLI collects the required simple confirmation. It never prepares an external source, queries
external VBUS current/voltage/power, or launches a browser.

`--power-class` is required and accepts exactly `pps3a` or `pps5a`. It is not an alias for
`--profile-mode`; `auto`, `65w`, and `100w` are rejected for this command.

The CLI remains a normal host process during a run. It does not hand its recorder, report
ownership, or reference comparison responsibility to `devd`; an interrupted CLI leaves the
firmware run safe but can make its candidate review-incomplete if the trace buffer expires.

## Host Reference Engine

```text
flux-purr thermal tune --engine host-reference --power-class pps5a --output-dir <directory>
```

`--engine host-reference` retains the current host-driven optimizer as an independent,
long-lived reference implementation. Existing legacy aliases and arguments remain mapped to
this engine during migration so current developer/HIL workflows continue to work. It may use
explicitly configured bench diagnostics in its own developer path; those diagnostics are not
part of the firmware-engine product contract or Web workflow.

The reference engine must remain executable and separately tested. Removing it, making it a
wrapper around the firmware optimizer, or deleting its legacy aliases requires explicit owner
approval.

## Comparison

```text
flux-purr thermal compare --firmware-bundle <directory-or-zip> \
  --reference-bundle <directory-or-zip> --output <comparison.json>
```

Comparison normalizes shared canonical input and ledger fields, then reports exactly one:

- `equivalent`: canonical target dispositions, selected candidate hashes and applicable gate
  results agree.
- `divergent`: sufficient complete evidence exists and one or more comparable decisions differ.
- `inconclusive`: evidence is incomplete, schemas cannot be aligned, or a comparison input is
  missing required ledger coverage.
- `not_run`: no reference bundle was requested or produced for this firmware run.

The result is diagnostic metadata in the report. It never changes firmware terminal state,
review seal, preview eligibility or save eligibility.

## Confirmation and Exit Semantics

Start, cancel and save are guarded by a simple yes/no interactive confirmation when the CLI is
attached to a terminal. No confirmation asks the operator to type a password, token, phrase or
secret. Non-interactive automation must use the project-standard explicit confirmation flag;
that flag confirms intent only and is not a capability credential.

The CLI exits nonzero for transport failures, Device rejection, incomplete recording, invalid
bundle output or a non-promotable requested save. A completed Device run with an unsealed local
trace is reported as `review_incomplete`, not as a hidden success. `divergent`, `inconclusive`
and `not_run` comparison outcomes are shown separately from the Device run outcome.

## Report Output

Both engines can write the common `thermal-tuning-v2` layout when they have enough data; the
firmware engine must do so for every terminal local archive. Legacy
`thermal-profile.accepted.json` is accepted only by import/reference compatibility commands and
is never emitted by the firmware-engine product command.

See [file-formats.md](./file-formats.md) for the required files and [control-plane.md](./control-plane.md)
for the Device protocol.
