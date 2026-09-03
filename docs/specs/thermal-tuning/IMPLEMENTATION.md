# Flux Purr 热控调优实现状态

> 当前有效契约以 [`SPEC.md`](./SPEC.md) 为准；本文件记录已落地覆盖和交付门槛。

## Current Status

- Implementation: firmware, transport, host tooling and Web surface are in progress; the
  complete evidence event contract is not yet implemented
- Lifecycle: active
- Delivery: local quality gates pass; hardware validation remains authorization-gated

## Implemented Surfaces

- `crates/thermal-tuning-core` provides the fixed-capacity `no_std` state machine,
  canonical fixed-point candidate encoding, bounded ladder, gate evaluation and rolling
  trace digest. `crates/thermal-tuning-wasm` exposes target order, candidate hashing and
  verification for browser replay.
- Firmware owns the live run through `ThermalTuningRuntime` and `MaintenanceRunArbiter`.
  USB JSONL, device LAN HTTP and DEVD expose the same `thermal_tuning_run_v1` snapshot and
  command model. Firmware journals only the compact start/terminal recovery projection;
  the bounded unacknowledged trace window, candidates and materialized tuning snapshots are
  explicitly allocated from the board's 2 MiB PSRAM without internal-RAM fallback. Internal
  RAM retains only real-time control state, pointers and compact metadata.
- The CLI has explicit `--engine firmware --power-class pps3a|pps5a` product execution and
  retains the independent `--engine host-reference` path plus legacy source/profile flags.
  The firmware runner writes the five-file `thermal-tuning-v2` archive without source or
  external VBUS operations. `thermal candidate preview|discard-preview|save` separately
  revalidates an archived firmware candidate against the Device before RAM preview or a
  second-confirmed EEPROM save. Report writers validate the canonical archive and rendered
  embedded JSON before emitting a `verified_bundle` receipt; `thermal report serve` is the
  loopback-only, process-owned path that performs a health and entry-page HTTP probe before
  publishing a temporary local report URL.
- Firmware now exposes the complete five-kind evidence union (`sample`, `phase_transition`,
  `candidate_trial`, `decision`, `safety`) with target/trial/candidate identity and canonical
  candidate bytes. CLI and Web recorders persist the global sequence independently and
  acknowledge only after durable host commit. Existing bundles made before this evidence
  contract remain visibly `review_incomplete`; missing fields are never inferred.
- The Web calibration workspace has a `热控调优` subtab, PPS segmented control, simple
  confirmation, trace acknowledgement/review gates, candidate preview/save/discard and
  IndexedDB plus offline ZIP export. Bridge, Web Serial and direct LAN use separate local
  transport paths and never communicate with the CLI.
- Unit and mock coverage exists for the core, firmware runtime, transport mappings, CLI
  parsing/runner and the Web card/recorder. Wasm generation is a prerequisite of Web
  typecheck/build/Storybook commands and generated output is ignored by Git.

## Planned Delivery Order

1. 创建独立的 Rust thermal-tuning core，冻结 canonical fixed-point data model、
   candidate hash、decision ledger 编码与跨 target golden vector。
2. 将 core 接入 firmware：维护仲裁、PPS class eligibility、run state machine、
   trace page/ack/seal、two-phase journal、preview/save gate 和 reset recovery。
3. 将 `thermal_tuning_run_v1` 映射到 USB JSONL、设备 LAN HTTP 和 DEVD Bridge；
   DEVD 只做请求/响应转换，不持有 run 或报告状态。
4. 将 CLI 分成 firmware host runner 与显式 `host-reference` engine，保留旧算法和
   非阻塞 comparison；产品 runner 不接触外部 VBUS telemetry。
5. 编译核心的 Wasm replay/verification binding，接入 Web 浏览器持久记录器与
   `thermal-tuning-v2` ZIP export。
6. 在校准页落地热控调优 subtab，覆盖三种 transport、简单确认、断连状态、候选
   preview/save/discard、Storybook 和视觉证据。
7. 更新旧控制平面规格、HTTP/CLI 文档与兼容导入路径，执行 mock/integration suite。
8. 仅在取得精确端口和主人写入授权后执行两个 PPS class 的 HIL；HIL receipt 不由
   mock 或其它 MCU 工具替代。

## Completion Conditions

- 所有 `SPEC.md` 实现里程碑均完成，并在规格索引中反映进度。
- 固件、native 与 Wasm 的 golden decision/hash 全量相同。
- 三种 Web transport 与 CLI firmware runner 可完成 mock run、完整 trace seal、
  candidate preview/save 和相同 report export。
- 旧 host-reference 可独立运行且 comparison 不影响设备 promotion。
- 设备 reset/trace gap/arbiter/PPS mismatch 等失败路径经过自动化覆盖；两类 PPS 的
  HIL 在授权后留有可审计 receipt。

## References

- [`SPEC.md`](./SPEC.md)
- [`HISTORY.md`](./HISTORY.md)
- [`control-plane.md`](./contracts/control-plane.md)
- [`cli.md`](./contracts/cli.md)
- [`file-formats.md`](./contracts/file-formats.md)
