# Thermal Tuning CLI Contract

## Firmware Engine

The product CLI path is a host recorder and report writer for the Device-owned engine:

```text
flux-purr thermal tune --engine firmware --power-class pps3a --output-dir <directory>
```

`--engine firmware` is the normal product mode. It discovers or consumes the existing chosen
Flux Purr transport, calls only `thermal_tuning_run_v1`, continuously pages/atomically records
trace events, sends `ack_trace` and `seal_review`, and writes a `thermal-tuning-v2` bundle.
The run command never promotes its candidate automatically. It never prepares an external
source, queries external VBUS current/voltage/power, or launches a browser.

`--power-class` is required and accepts exactly `pps3a` or `pps5a`. It is not an alias for
`--profile-mode`; `auto`, `65w`, and `100w` are rejected for this command.

The CLI remains a normal host process during a run. It does not hand its recorder, report
ownership, or reference comparison responsibility to `devd`; an interrupted CLI leaves the
firmware run safe but can make its candidate review-incomplete if the trace buffer expires.
The runner reads at most eight events per page, retries a short USB/bridge read failure a bounded
number of times, and treats a Device `trace_gap` as a safety workflow: archive the returned tail,
do not ack or seal it, cancel a still-running run with its returned runId, drain the remaining
tail, and write an explicit incomplete five-file bundle.

## Firmware Candidate Promotion

```text
flux-purr thermal candidate preview --device <device> --bundle-dir <directory>
flux-purr thermal candidate discard-preview --device <device> --bundle-dir <directory>
flux-purr thermal candidate save --device <device> --bundle-dir <directory> --confirm
```

These commands accept only the `run.bundle.json` from a firmware `thermal-tuning-v2` archive.
Before acquiring a Device lease they verify the archived completed/review-complete state,
`pps3a|pps5a` class, canonical profile bytes and candidate SHA-256. With the lease, they reread
the Device snapshot and require the exact `runId + candidateId + candidateHash + powerClass`
identity before issuing any candidate operation.

`preview` writes only the Device RAM bank and leaves heating disabled. `discard-preview` restores
the prior RAM bank. `save` performs the required preview when the candidate is `ready`, then
persists that exact `previewed` candidate to the matching EEPROM bank. It has its own simple
yes/no confirmation, separate from start; non-interactive use requires `--confirm` and never a
password, token or secret. A `saved` candidate is reported idempotently and no new write is
issued.

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
