# 规格（Spec）总览

本目录保存按主题组织的长期规格。每个主题使用稳定的 kebab-case slug，并包含 `SPEC.md`、`IMPLEMENTATION.md` 和 `HISTORY.md`；实现覆盖与局部演进分别收敛到 companion 文档。

旧版 ID 仅在对应主题的 `HISTORY.md` 中作为迁移兼容信息保留，不再作为目录、索引或当前引用的一部分。

## 目录与命名规则

- 每个主题一个目录：`docs/specs/<topic>/`。
- `<topic>` 使用小写 kebab-case，并作为主题的稳定身份。
- `SPEC.md` 描述背景、范围、需求、行为、契约、验收、风险和参考资料。
- `IMPLEMENTATION.md` 描述当前实现覆盖、验证和剩余缺口。
- `HISTORY.md` 描述主题生命周期、历史身份、决策和局部替代边界。

## Lifecycle

- `active`：当前仍是维护中的主题合同。
- `archived`：主题作为已完成的历史基线保留，不再驱动新的实现工作。
- `superseded`：仅用于整个主题被另一个主题完全取代；局部替代写在 `HISTORY.md`，不在 catalog 中伪造 successor。

## Index

<!-- Legacy ID index retained only for historical reference; the canonical index uses stable topic slugs.

## Legacy Index

| ID   | Title | Status | Spec | Last | Notes |
|-----:|-------|--------|------|------|-------|
| - | FUSB302B 双硬件 PD Sink | 部分完成（2/3） | `fusb302b-dual-pd-sink/SPEC.md` | 2026-08-27 | Public `fusb302` PHY integration, read-only `0x9x` identification, `5V..21V` PPS runtime dispatch with fixed-PDO fallback, contract-capped PWM, status contract, and netlist baseline are present; PPS `20V/3A` and `20V/5A` source HIL remains gated |
| 233y7 | Flux Purr S3FH4R2 + CH224Q 直连前面板基线（移除 CH442E / TCA6408A） | 已完成 | `233y7-c3-ch224q-ch442e-frontpanel/SPEC.md` | 2026-04-22 | Baseline updated for RGB status LED PWM on GPIO39/38/37 in addition to the frozen S3FH4R2 direct panel wiring |
| n6csh | Flux Purr 初始化（Hooks + Storybook + shadcn + UI UX Pro Max） | 已完成 | `n6csh-flux-purr-init/SPEC.md` | 2026-03-02 | Local PR-ready（未 push / 未建 PR） |
| 744yg | PD Mini加热台二开资料采集与基础文档 | 已完成 | `744yg-mini-hotplate-doc-baseline/SPEC.md` | 2026-03-03 | Research: [mini-hotplate](../research/mini-hotplate/README.md) |
| 8tesd | Flux Purr S3 风扇循环调速 bring-up | 已完成 | `8tesd-s3-fan-cycle-bringup/SPEC.md` | 2026-04-09 | PR #4 |
| 223uj | Flux Purr 160×50 前面板 UI 契约 | 已完成 | `223uj-frontpanel-ui-contract/SPEC.md` | 2026-04-21 | Visual baseline retained; runtime truth-source for heater/fan/dashboard is superseded by #q2aw6 |
| vmekj | Flux Purr S3 GC9D01 异步 SPI 显示 bring-up 与启动后界面轮播 | 已完成 | `vmekj-s3-gc9d01-display-bringup/SPEC.md` | 2026-04-13 | Orientation/colors confirmed; runtime behavior later superseded by #fk3u7 while display bring-up baseline stays canonical |
| fk3u7 | Flux Purr 前面板五向输入与交互导航 | 已完成 | `fk3u7-frontpanel-input-interaction/SPEC.md` | 2026-04-21 | Key Test mapping + dashboard/menu navigation remain canonical; heater/fan runtime semantics are superseded by #q2aw6 |
| jb85u | Release 失败通知接入 | 已完成 | `jb85u-release-failure-telegram-alerts/SPEC.md` | 2026-09-01 | Oidrune OIDC reusable workflow pinned by SHA, caller-owned release/smoke summaries, and no Telegram secret wiring |
| q2aw6 | Flux Purr 正式 PID 加热闭环与前面板运行态同步 | 已完成 | `q2aw6-heater-pid-frontpanel-runtime/SPEC.md` | 2026-04-21 | PR #11 |
| 22222 | Flux Purr Worktree Bootstrap | 已完成 | `22222-worktree-bootstrap/SPEC.md` | 2026-06-24 | Shared hooks + linked worktree auto bootstrap + required smoke gate |
| v5k2p | 双版本风扇 PCB 方案（5V / 12V） | 已完成 | `v5k2p-dual-fan-pcb-variants/SPEC.md` | 2026-04-10 | PR #6 |
| kht7p | Flux Purr 7.0cm 3.2Ω 加热板变体 | 已完成 | `kht7p-heater-7p0-3p2-variant/SPEC.md` | 2026-05-31 | `heater-7p0-3p2` Gerber archived |
| 3dczp | Flux Purr 5.6cm 外壳模型归档 | active | `3dczp-enclosure-5p6cm-models/SPEC.md` | 2026-05-31 | Print-ready STL assets tracked through Git LFS |
| 35bta | Flux Purr EEPROM 记忆配置 | 已完成 | `35bta-eeprom-memory-config/SPEC.md` | - | EEPROM-only persistence; MCU-internal configuration fallback and migration are unsupported |
| r9k3m | Flux Purr PR 标签发布与主分支保护 | 已完成 | `r9k3m-pr-label-release-protection/SPEC.md` | 2026-04-27 | Label-driven release intent, PR-local version preparation, and quality-gates declaration |
| hhwq8 | Flux Purr 热控 Bench Web Demo | active | `hhwq8-web-control-plane-demo/SPEC.md` | 2026-05-23 | Industrial mock thermal bench tool for the #27 control-plane architecture |
| m8r4q | Flux Purr 真实控制平面运行时 | active | `m8r4q-real-control-plane-runtime/SPEC.md` | 2026-05-29 | Web + firmware + native devd real transport contract |
| jt8r2 | Flux Purr ADC 校准控制面 | 已完成 | `jt8r2-adc-calibration-control-plane/SPEC.md` | 2026-06-02 | RTD/VIN ADC calibration with persisted draft/active packages |
| web-firmware-install-recovery | Flux Purr Web 固件安装与恢复 | 已完成 | `web-firmware-install-recovery/SPEC.md` | 2026-08-15 | Unified integrity-catalog bundle handling across devd and Browser Web Serial |
| - | Flux Purr 单一产品版本源 | 已实现 | `version-source/SPEC.md` | 2026-08-29 | Root `VERSION`, PR-local preparation, one-product-merge/one-release sequencing, and the release-completion gate |
| - | Flux Purr 蜂鸣器单输出 Cue 仲裁 | 已完成 | `buzzer-cue-arbitration/SPEC.md` | 2026-09-02 | Single-output priority, safety suppression, coalesced feedback, host-side verification, and feature-gated native USB/devd diagnostics; [ADR 0006](../adr/0006-single-output-buzzer-cue-arbitration.md) |
| - | Firmware update and developer flash | 已完成 | `firmware-update-and-developer-flash/SPEC.md` | - | Explicit-port update, direct local ELF flash/recover, local CBOR control, and EEPROM-only persistence boundary |
-->

## Index
| Topic | Lifecycle | Implementation | Spec | Successor | Notes |
| --- | --- | --- | --- | --- | --- |
| adc-calibration-control-plane | active | `adc-calibration-control-plane/IMPLEMENTATION.md` | `adc-calibration-control-plane/SPEC.md` | - | RTD/VIN calibration state, slots, and thermal-plant calibration contract |
| buzzer-cue-arbitration | active | `buzzer-cue-arbitration/IMPLEMENTATION.md` | `buzzer-cue-arbitration/SPEC.md` | - | Single-output priority, safety suppression, and diagnostic cues |
| dual-fan-pcb-variants | active | `dual-fan-pcb-variants/IMPLEMENTATION.md` | `dual-fan-pcb-variants/SPEC.md` | - | 5 V and 12 V board variants with a voltage-agnostic firmware contract |
| eeprom-memory-config | active | `eeprom-memory-config/IMPLEMENTATION.md` | `eeprom-memory-config/SPEC.md` | - | M24C64 EEPROM-only persistence; MCU-internal configuration fallback and migration are unsupported |
| enclosure-5p6cm-models | active | `enclosure-5p6cm-models/IMPLEMENTATION.md` | `enclosure-5p6cm-models/SPEC.md` | - | Tracked 5.6 cm enclosure model baseline |
| flux-purr-init | archived | `flux-purr-init/IMPLEMENTATION.md` | `flux-purr-init/SPEC.md` | - | Repository bootstrap and initial toolchain decisions are historical |
| frontpanel-input-interaction | active | `frontpanel-input-interaction/IMPLEMENTATION.md` | `frontpanel-input-interaction/SPEC.md` | - | Five-way input, gestures, Key Test, and navigation truth source |
| frontpanel-ui-contract | active | `frontpanel-ui-contract/IMPLEMENTATION.md` | `frontpanel-ui-contract/SPEC.md` | - | 160x50 visual tokens, layout, and display-state baseline |
| fusb302b-dual-pd-sink | active | `fusb302b-dual-pd-sink/IMPLEMENTATION.md` | `fusb302b-dual-pd-sink/SPEC.md` | - | Dual FUSB302B PD sink and PPS/fixed-PDO contract |
| heater-7p0-3p2-variant | active | `heater-7p0-3p2-variant/IMPLEMENTATION.md` | `heater-7p0-3p2-variant/SPEC.md` | - | 7.0 cm, 3.2 ohm heater hardware variant |
| heater-pid-frontpanel-runtime | active | `heater-pid-frontpanel-runtime/IMPLEMENTATION.md` | `heater-pid-frontpanel-runtime/SPEC.md` | - | Heater PID, fan policy, protection, and dashboard runtime truth source |
| mini-hotplate-doc-baseline | archived | `mini-hotplate-doc-baseline/IMPLEMENTATION.md` | `mini-hotplate-doc-baseline/SPEC.md` | - | Source-collection and evidence baseline is complete |
| pr-label-release-protection | active | `pr-label-release-protection/IMPLEMENTATION.md` | `pr-label-release-protection/SPEC.md` | - | Label-driven release intent and branch protection policy |
| real-control-plane-runtime | active | `real-control-plane-runtime/IMPLEMENTATION.md` | `real-control-plane-runtime/SPEC.md` | - | Web, firmware, and native devd real transport contract |
| release-failure-telegram-alerts | active | `release-failure-telegram-alerts/IMPLEMENTATION.md` | `release-failure-telegram-alerts/SPEC.md` | - | Release failure notification workflow and recovery context |
| s3-ch224q-frontpanel-baseline | active | `s3-ch224q-frontpanel-baseline/IMPLEMENTATION.md` | `s3-ch224q-frontpanel-baseline/SPEC.md` | - | ESP32-S3 direct-panel and CH224Q hardware baseline |
| s3-fan-cycle-bringup | archived | `s3-fan-cycle-bringup/IMPLEMENTATION.md` | `s3-fan-cycle-bringup/SPEC.md` | - | Historical four-phase fan bring-up state machine |
| s3-gc9d01-display-bringup | active | `s3-gc9d01-display-bringup/IMPLEMENTATION.md` | `s3-gc9d01-display-bringup/SPEC.md` | - | GC9D01 async display driver and host-preview baseline |
| version-source | active | `version-source/IMPLEMENTATION.md` | `version-source/SPEC.md` | - | Single product version source and release sequencing |
| web-control-plane-demo | active | `web-control-plane-demo/IMPLEMENTATION.md` | `web-control-plane-demo/SPEC.md` | - | Mock-first thermal bench Web console |
| web-firmware-install-recovery | active | `web-firmware-install-recovery/IMPLEMENTATION.md` | `web-firmware-install-recovery/SPEC.md` | - | Unified devd and Browser Web Serial firmware workbench |
| worktree-bootstrap | active | `worktree-bootstrap/IMPLEMENTATION.md` | `worktree-bootstrap/SPEC.md` | - | Linked worktree bootstrap and shared Git hooks |
| firmware-update-and-developer-flash | active | `firmware-update-and-developer-flash/IMPLEMENTATION.md` | `firmware-update-and-developer-flash/SPEC.md` | - | Explicit-port update, direct local ELF flash/recover, local CBOR control, and EEPROM-only persistence boundary |
