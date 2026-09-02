# Thermal Tuning Control-plane Contract

## Capability

设备 identity/status capability 中以字符串 `thermal_tuning_run_v1` 声明本协议。其
对应 capability detail 必须包含：

```json
{
  "id": "thermal_tuning_run_v1",
  "evidenceSchema": "thermal_tuning_evidence_v2",
  "supportedPowerClasses": ["pps3a", "pps5a"],
  "targetScheduleC": [60, 240, 140, 100, 80, 120, 180, 160, 220],
  "physicalTargetsC": [60, 80, 100, 120, 140, 160, 180, 220, 240],
  "trace": {
    "paged": true,
    "acknowledged": true,
    "sealedReview": true,
    "bufferCapacity": 0
  },
  "candidatePromotion": true
}
```

`bufferCapacity` 是设备实际固定容量，且大于零。缺少 capability 或 detail 的设备不
支持本协议；客户端必须显示不兼容，不能从 `thermalProfileMode` 推断支持性。
`evidenceSchema` 必须精确为 `thermal_tuning_evidence_v2`，表示设备会输出本合同定义的
完整 sample、phase transition、candidate trial、decision 与 safety 证据。缺少该值或
值不匹配时，客户端可以只读归档设备实际返回的数据，但不得将 run 标记为 review
complete、生成可保存候选或声称报告与正式调优报告等价。

## Transport Mapping

三个 transport 共享同一 command/response model。

| Transport | Read | Command |
| --- | --- | --- |
| Device LAN HTTP | `GET /calibration/thermal-tuning/run?afterSequence=<u64>&limit=<u16>` | `POST /calibration/thermal-tuning/run` |
| DEVD Bridge | `GET /api/v1/devices/{deviceId}/calibration/thermal-tuning/run?lease_id=<id>&afterSequence=<u64>&limit=<u16>` | `POST /api/v1/devices/{deviceId}/calibration/thermal-tuning/run` |
| USB JSONL | `{"type":"thermal_tuning_run","op":"get",...}` | `{"type":"thermal_tuning_run","op":<command>,...}` |

DEVD 必须逐字段转发 Device semantics、status 和 error envelope；不得缓存 trace、
生成 report、启动 CLI、操作 source 或替换 command 的 owner。LAN 与 Bridge 沿用既有
pairing/lease 规则；Web Serial 沿用已授权端口规则。协议不引入密码、口令、一次性
approval token 或额外身份步骤。

## Read Snapshot

`get` 返回 `ThermalTuningRunSnapshot`。分页 cursor 是排他的 `afterSequence`；首次
读取使用 `afterSequence: null`。响应至少包含：

```json
{
  "schema": "thermal_tuning_run_v1",
  "run": {
    "runId": "opaque",
    "state": "idle|running|terminal",
    "powerClass": "pps3a|pps5a|null",
    "phase": "idle|cooldown_wait|scout|retune|hold_confirm|terminal",
    "currentTargetC": 140,
    "targetProgress": {
      "acceptedC": [60, 240],
      "failedC": [],
      "skippedC": []
    },
    "terminalDisposition": "completed|failed|cancelled|budget_exhausted|safety_disarmed|review_incomplete|interrupted_reset|null",
    "eligibility": {
      "ready": true,
      "reasons": [],
      "activeOwner": null
    },
    "review": {
      "state": "not_applicable|recording|awaiting_seal|complete|incomplete",
      "reason": "trace_gap|null",
      "acknowledgedThrough": null,
      "terminalSequence": 0,
      "traceDigest": "hex|null"
    },
    "candidate": {
      "candidateId": "opaque",
      "candidateHash": "hex",
      "canonicalProfileHex": "hex|null",
      "powerClass": "pps3a",
      "promotionState": "unavailable|awaiting_review|ready|previewed|saved|expired"
    },
    "journal": {
      "lastRunId": "opaque|null",
      "lastDisposition": "interrupted_reset|null",
      "resetReason": "system_brownout|null"
    }
  },
  "page": {
    "earliestSequence": 0,
    "emittedThrough": 0,
    "nextAfterSequence": 0,
    "acknowledgedThrough": null,
    "digestThroughPage": "hex|null",
    "events": []
  }
}
```

`currentTargetC`, `candidate`, and live review details can be `null` when not applicable.
The terminal journal is a compact recovery projection, never a substitute for raw events.

Every page event has a global `sequence`, monotonic `elapsedMs`, canonical `kind`, and
fixed-point payload. The allowed kinds are `sample`, `phase_transition`, `candidate_trial`,
`decision`, and `safety`.

`sample` payloads contain `targetC`, `trialIndex`, candidate identity/parameter reference,
`temperatureCentiC`, `vinMv`, `ppsContractMv`, `ppsContractMa`, `heaterOutputPermille`,
measurement validity and phase. They must not contain external VBUS measurements.
`phase_transition` records old/new phase and reason. `candidate_trial` records the complete
canonical fixed-point candidate, trial index, start/end sequence and time, and sample range. A
candidate trial starts only after its `cooldown_wait` precondition has reached `target-5°C` and
the firmware enters `scout`; prior cooldown samples remain target-level safety evidence and are
outside that candidate trial's sample range and dynamic-settle score.
Each candidate's `scout` interval is timed from its own start boundary and lasts at least five
seconds. Its `warmup_complete` gate may only be satisfied by a nonzero actual heater-output
sample inside that candidate's `scout` interval; neither cooldown nor a prior candidate contributes scoring
measurements or warmup state.
`decision` records the complete candidate/score/gate/target state, freeze and interval result.
`safety` records a safety fault and disarm reason. Preview, discard and save occur after terminal
trace sealing, so their normal command responses carry applied hash, persistent revision and
outcome. Hosts append those responses to the candidate file as post-seal promotion receipts;
they are never inserted into the sealed trace or its rolling digest.
The union of these events is the only source from which the host report may reconstruct a run;
renderers must reject a completed/candidate-ready report when any required event is absent.

`emittedThrough` is the latest event emitted by the Device, `nextAfterSequence` is the cursor
for the next page, and `digestThroughPage` is the canonical rolling digest at the last returned
event. An empty page has `digestThroughPage: null`. The Device retains enough per-event digest
state in its bounded buffer to validate an `ack_trace` before that event is evicted.

The buffer is a PSRAM-backed unacknowledged retransmission window, not whole-run storage.
CLI persists sample and non-sample events into the two NDJSON files and completes filesystem
data synchronization before ack. Web merges the global sequence into its run record and waits
for the IndexedDB read-write transaction to complete before ack. Normal clients perform this
automatically for every page; acknowledgement is not a user action. DEVD only proxies these
reads and commands and never becomes the recorder.

If `afterSequence` predates `earliestSequence - 1`, the response has error `trace_gap` and
includes the available range. A client must permanently mark that archive incomplete;
it must not fill the missing range with interpolated data.

## Commands

All commands are sent to the command endpoint/frame with an `op` and `requestId`. Existing
mutating authority applies to `start`, `cancel`, `ack_trace`, `seal_review`, `preview`,
`discard_preview`, and `save`; read operations remain read-only.

### `start`

```json
{
  "op": "start",
  "requestId": "client-generated",
  "powerClass": "pps3a"
}
```

The command creates a new run only after Maintenance Run Arbiter and all eligibility checks
pass. The device derives its PPS contract itself. It rejects any class other than `pps3a` or
`pps5a` and never changes the requested class. A UI simple confirmation happens before this
command; it is not an API credential.

### `cancel`

```json
{ "op": "cancel", "requestId": "client-generated", "runId": "opaque" }
```

Only the active run can be cancelled. Firmware disarms, records a terminal disposition and
does not create a promotable candidate. Clients should ask for a simple confirmation before
sending it.

### `ack_trace`

```json
{
  "op": "ack_trace",
  "requestId": "client-generated",
  "runId": "opaque",
  "throughSequence": 347,
  "traceDigest": "hex"
}
```

The client sends this only after atomically persisting every event from the previous ack
through `throughSequence`. Firmware accepts only the next contiguous range and matching
canonical rolling digest. It retains no host filesystem or browser data. Failure to keep up
before ring-buffer eviction sets `trace_gap`; no later ack can clear it.

### `seal_review`

```json
{
  "op": "seal_review",
  "requestId": "client-generated",
  "runId": "opaque",
  "throughSequence": 512,
  "traceDigest": "hex"
}
```

This is valid only for a terminal run whose acknowledged range and digest exactly equal its
terminal trace. Firmware then marks the in-RAM candidate `ready`; it does not write a third
journal record. `trace_gap`, terminal failure, cancellation, reset recovery or a different
digest returns `review_incomplete`.

### `preview`, `discard_preview`, and `save`

```json
{
  "op": "preview",
  "requestId": "client-generated",
  "runId": "opaque",
  "candidateId": "opaque",
  "candidateHash": "hex",
  "powerClass": "pps5a"
}
```

`preview` requires an in-RAM `ready` candidate and applies only that candidate to the matching
RAM bank. It reports the applied hash and leaves `heaterEnabled` false. `discard_preview`
requires the same run/candidate identity and restores the pre-preview RAM bank without an
EEPROM write. `save` repeats the four identity fields, requires `previewed`, verifies the
currently applied hash, and persists only the matching bank. A second UI/CLI simple
confirmation is required before `save`; the protocol itself carries no password/token.

## Errors

The normal control-plane error envelope carries one stable code and optional structured data:

| Code | Meaning |
| --- | --- |
| `thermal_tuning_unsupported` | Capability absent or firmware schema incompatible |
| `tuning_busy` | Maintenance Run Arbiter has an active owner |
| `tuning_ineligible` | Model, curve, PPS, idle, measurement or safety prerequisite failed |
| `tuning_power_class_unavailable` | Explicit class cannot be maintained; no fallback occurs |
| `trace_gap` | Event coverage was lost or request cursor is too old |
| `review_incomplete` | Archive cannot be sealed or terminal run is not promotable |
| `candidate_mismatch` | Run, ID, hash, class or RAM readback differ |
| `candidate_not_previewed` | Save requested before a successful preview |
| `candidate_expired` | Candidate was lost after reset/new run/discard boundary |
| `tuning_run_not_active` | Cancel/ack targets no matching active run |

## Compatibility

`thermal_plant_run`, generic calibration jobs, and runtime profile mode endpoints retain their
own schemas. They must not accept a `thermal_tuning_run` command. Capability absence is an
explicit incompatibility, not a request to call the legacy CLI algorithm.
