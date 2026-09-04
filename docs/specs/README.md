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

| Topic | Lifecycle | Implementation | Spec | Successor | Notes |
| --- | --- | --- | --- | --- | --- |
| adc-calibration-control-plane | active | `adc-calibration-control-plane/IMPLEMENTATION.md` | `adc-calibration-control-plane/SPEC.md` | - | RTD/VIN calibration state, slots, and thermal-plant calibration contract |
| buzzer-cue-arbitration | active | `buzzer-cue-arbitration/IMPLEMENTATION.md` | `buzzer-cue-arbitration/SPEC.md` | - | Single-output priority, safety suppression, and diagnostic cues |
| dual-fan-pcb-variants | active | `dual-fan-pcb-variants/IMPLEMENTATION.md` | `dual-fan-pcb-variants/SPEC.md` | - | 5 V and 12 V board variants with a voltage-agnostic firmware contract |
| eeprom-memory-config | active | `eeprom-memory-config/IMPLEMENTATION.md` | `eeprom-memory-config/SPEC.md` | - | M24C64 persistence, migration, and calibration payloads |
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
