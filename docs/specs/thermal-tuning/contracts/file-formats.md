# Thermal Tuning Report Format

## `thermal-tuning-v2`

`thermal-tuning-v2` is a portable directory format. The Web exports the exact same directory
contents in a ZIP; CLI writes a directory and may optionally package an identical ZIP. All
files are UTF-8 and self-contained: `index.html` must render without network access.

| File | Required content |
| --- | --- |
| `index.html` | Offline human-readable report with summary, nine-target progress, candidate state, trace integrity, decision details and optional reference comparison |
| `run.bundle.json` | Run identity, device/firmware identity, selected power class, terminal/review dispositions, capability, canonical metadata and file digests |
| `samples.ndjson` | Every archived `sample` trace event in global sequence order |
| `thermal-profile.candidate.json` | Canonical candidate profile, candidate ID/hash, bank, promotion state and preview/save receipts when present |
| `decision-ledger.ndjson` | Every archived non-sample trace event in global sequence order |

The five files are mandatory even for a failed or review-incomplete run. Incomplete reports
carry their available contiguous events and explicit integrity disposition; they never invent
or interpolate omitted samples.

## `run.bundle.json`

The top-level schema value is exactly `"thermal-tuning-v2"`. Required fields include:

```json
{
  "schema": "thermal-tuning-v2",
  "runId": "opaque",
  "engine": "firmware|host-reference",
  "powerClass": "pps3a|pps5a",
  "physicalTargetsC": [60, 80, 100, 120, 140, 160, 180, 220, 240],
  "executionOrderC": [60, 240, 140, 100, 80, 120, 180, 160, 220],
  "terminalDisposition": "completed|failed|cancelled|budget_exhausted|safety_disarmed|review_incomplete|interrupted_reset",
  "reviewDisposition": "complete|incomplete|not_applicable",
  "trace": {
    "firstSequence": 0,
    "lastSequence": 0,
    "complete": true,
    "digest": "hex",
    "gap": null
  },
  "candidate": {
    "candidateId": "opaque|null",
    "candidateHash": "hex|null",
    "promotionState": "unavailable|awaiting_review|ready|previewed|saved|expired"
  },
  "referenceComparison": "equivalent|divergent|inconclusive|not_run",
  "files": {}
}
```

`engine=firmware` identifies the Device-authoritative product flow. `engine=host-reference`
is compatibility/diagnostic evidence and never claims that the Device accepted its candidate.
The bundle records transport type for traceability but never records source-control secrets,
external VBUS measurements, browser pairing secrets or CLI credentials.

## NDJSON Event Rules

Each line represents one canonical event and includes `sequence`, `elapsedMs`, `kind`,
`targetC` when applicable, and a fixed-point payload. The five allowed event kinds are
`sample`, `phase_transition`, `candidate_trial`, `decision`, and `safety`.
`samples.ndjson` contains the sample events; `decision-ledger.ndjson`
contains the non-sample events in sequence order. The union must cover every integer sequence
from `firstSequence` through `lastSequence` when `trace.complete=true`.
The two files do not have independent sequence spaces. A renderer merges them by `sequence`
before checking digest coverage, paired candidate-trial boundaries, nine-target coverage and
terminal seal.

Sample payload fields are device-local `targetC`, `trialIndex`, candidate identity/reference,
`temperatureCentiC`, `vinMv`, `ppsContractMv`, `ppsContractMa`, `heaterOutputPermille`,
measurement validity and core phase. Candidate-trial payloads contain complete canonical
candidate bytes, trial boundaries and sample range. Decision payload fields include candidate
canonical bytes/hash, the complete score vector, every hard-gate outcome, interval
dependency/freeze result and terminal reason. Safety payloads are required when those
transitions occur. Post-seal preview/discard/save responses are stored as promotion receipts in
`thermal-profile.candidate.json` and are not part of the sealed sequence range. Fields are
emitted in canonical JSON ordering for digest
verification; report renderers must not use display-rounded values for replay.

## Candidate File

`thermal-profile.candidate.json` contains the banked profile points in canonical fixed-point
form plus `runId`, `candidateId`, `candidateHash`, `canonicalProfileHex`, `powerClass`, core version, review state and
promotion receipts. A preview receipt includes applied RAM hash; a save receipt includes the
matching persistent bank revision/hash. It does not contain an auto-resolved profile mode.

For terminal runs without a candidate, the file remains present and carries a null candidate
with the terminal reason. For `interrupted_reset`, it must state that no promotable candidate
survived the reset.

## Web Storage and ZIP

Web stores raw events and report metadata locally under a stable `deviceId + runId` key. It
must make a durable write before acknowledging a page to firmware. Its ZIP contains precisely
the five required root files, with no dependency on a server or CLI process. Storage quota,
transaction failure or detected gap forces `reviewDisposition=incomplete` and is visible in
both report and UI.

## Report Rendering

The target-card verdict, displayed hard metrics and default response chart describe one
candidate trial. When an adopted trial exists, it is the default and the report labels its
trial number and adopted state. Other executed trials remain individually selectable so a
rejected candidate cannot be mistaken for the adopted result.

A renderer that offers a target-level multi-trial overview orders samples by the Device-global
`elapsedMs` and breaks every line at a `trialIndex` transition. It must never concatenate
trial-local clocks or draw a segment across a candidate boundary.

Charts preserve physical dimensions: temperature, heater output, voltage and current use
separate plots or explicitly labelled independent axes. A renderer must not use a hidden
multiplier to overlay distinct units. Device-reported `ppsContractMa` is shown only as the
PPS contract safety ceiling, not as external VBUS-current telemetry. Heater output is bounded
to the physical `0–100%` range.

## Legacy Import

`thermal-profile.accepted.json` remains an import-only compatibility artifact for historical
CLI/reference analysis. It is not a `thermal-tuning-v2` required file, cannot provide a
complete trace, and cannot be used to preview/save a new firmware candidate.
