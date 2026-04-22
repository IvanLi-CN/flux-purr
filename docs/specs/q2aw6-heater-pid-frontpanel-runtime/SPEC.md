# Flux Purr 正式 PID 加热闭环与前面板运行态同步（#q2aw6）

## 状态

- Status: 已完成
- Created: 2026-04-21
- Last: 2026-04-21

## 背景 / 问题陈述

- 当前 `flux-purr` 已经完成前面板输入、RTD 读取、CH224Q 20V 请求与 heater/fan bring-up，但 heater 仍是固定占空比测试波形，无法按设定温度稳定工作。
- `#223uj` 与 `#fk3u7` 已冻结前面板视觉和五向输入基线，但 Dashboard 的 heater/fan 语义仍残留 mock 时代口径，和现在的真实运行态不一致。
- 若不把温控、故障保护、风扇联动和 UI 真相源一次收口，后续板级调试会持续混淆“显示状态”“控制状态”“保护状态”三套口径。

## 目标 / 非目标

### Goals

- 把 `GPIO47` 固定占空比加热替换为按 `target_temp_c` 驱动的正式 PID PWM 闭环。
- 让前面板稳定显示实时温度、设定温度、实际风扇状态与实际 heater 输出强度。
- 冻结正式温控包线：`target 0~400°C`、`fan on >=360°C`、`fan off <340°C`、`hard cutoff >=420°C`。
- 冻结故障策略：RTD 开路/短路、ADC 读失败、硬超温进入 heater fault-latch；PD 状态仅记录，不参与 heater 锁死或闭锁。
- 产出 merge-ready 所需的 spec、视觉证据、板级验证与 review 收敛材料。

### Non-goals

- 不提供运行时 PID 参数调节入口。
- 不实现按 VIN / PD 档位的动态 duty 补偿。
- 不修改外部 HTTP / RPC / 持久化契约。
- 不扩展新的前面板菜单层级或联网业务逻辑。

## 范围（Scope）

### In scope

- `firmware/src/bin/flux_purr.rs`
- `firmware/src/frontpanel/**`
- `firmware/src/bin/frontpanel_preview.rs`
- `firmware/README.md`
- `docs/specs/q2aw6-heater-pid-frontpanel-runtime/**`
- `docs/specs/fk3u7-frontpanel-input-interaction/SPEC.md`
- `docs/specs/223uj-frontpanel-ui-contract/SPEC.md`
- `docs/specs/README.md`

### Out of scope

- Web 控制台、HTTP API、Wi‑Fi 配置写回
- 多电压 / 多功率档位与自动 PD 策略切换
- RTD 额外校准界面或外部校准协议

## 需求（Requirements）

### MUST

- heater PWM 频率固定为 `2 kHz`，控制周期固定为 `1 Hz`。
- 目标温度与 preset 写入都必须 clamp 到 `0~400°C`。
- PID 必须固定使用内置常量参数，并包含积分限幅、设定变化重置积分、导数对测量值求差。
- RTD 开路、短路、ADC 读失败、`temp >= 420°C` 时，heater 必须立即关断并进入 fault-latch。
- fault-latch 期间 heater 不得自动恢复；故障解除后必须由用户再次短按中键重臂。
- CH224Q 仍在启动时请求 `20V`；PD 状态变化只允许进入日志/状态观测，不得触发 heater 锁死或自动关断。
- 风扇默认关闭；`temp >= 360°C` 强制开启，降到 `<340°C` 才允许关闭。
- Dashboard 中键短按只切 heater arm；中键双击不得再影响风扇运行态。
- Dashboard 左侧必须显示实时温度；右上必须显示 `SET` 与设定温度；`FAN` 必须显示实际运行态；底部条形必须绑定实际 heater 输出 duty。
- defmt 日志必须覆盖 RTD 读数、PID 输入/输出、fault 原因、fan hysteresis 状态与 PD 状态变化。

### SHOULD

- fault-latch 的原因标签应保持稳定，便于 monitor 与后续 review 收敛。
- 初始 UI 应在第一次有效 RTD 样本后就显示实际温度，而不是长时间保留 bring-up 默认值。
- preview 与板级运行态的 Dashboard 布局、文案与颜色层级应保持一致。

### COULD

- 后续在同一条 PID 日志上扩展功率估算或 duty limit 观察字段。

## 功能与行为规格（Functional / Behavior Spec）

### Core flows

- 启动后仍先请求 `20V`，随后初始化 RTD、heater PWM、fan 运行态和前面板 UI。
- 用户短按中键后，heater 进入 arm 状态；若无 fault-latch，则 PID 开始按 `target_temp_c - current_temp_c` 驱动 duty。
- 当实时温度靠近设定温度时，heater duty 应明显下降；超过设定温度时 duty 应继续收敛到低值或 0。
- 风扇不跟随 heater arm 自动开启；只有温度达到阈值时才进入全速运行。
- PD 状态只做观测：即使 PD 丢失或降档，也不自动清空 `heater_enabled`，只在日志中体现。

### Edge cases / errors

- 首次 RTD 采样失败时，heater 必须保持关断，直到后续有效样本恢复且用户重新 arm。
- fault-latch 期间若用户再次短按中键：
  - 当前 fault 仍存在时，必须拒绝重臂并保持 `heater_enabled=false`
  - 当前 fault 已消失时，允许清除 latch 并重新进入 arm
- overtemp fault 后，若温度仍高于风扇回差阈值，风扇必须继续运行直到安全回落。
- 双击中键在 v1 中保留为无副作用事件，不得污染 heater/fan 真相源。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `FrontPanelUiState.heater_output_percent` | Rust state model | internal | New | None | firmware | runtime / preview / render tests | Dashboard 底部输出条真相源 |
| `FrontPanelUiState.fan_enabled` | Rust state model | internal | Modify | None | firmware | runtime / preview / render tests | 由 mock flag 改为实际 fan runtime state |
| `HeaterController` | Rust runtime helper | internal | New | None | firmware | main runtime / unit tests | PID + fault-latch 状态机 |

### 契约文档（按 Kind 拆分）

None

## 验收标准（Acceptance Criteria）

- Given 固件刚启动，When RTD 已有有效样本，Then Dashboard 左侧显示实时温度，右上显示 `SET <target>`，`FAN` 显示实际 fan runtime state，heater 默认 `arm=false`。
- Given Dashboard，When 用户短按中键，Then 只切换 heater arm，不再改变 fan state；When 双击中键，Then 不产生 heater/fan 副作用；When 长按中键，Then 仍进入菜单。
- Given heater 已 arm 且无 fault，When 实时温度低于设定温度，Then heater duty 上升；When 接近设定温度，Then heater duty 收敛下降，而不是固定 `50%`。
- Given RTD 开路、短路、ADC 读失败或 `temp >= 420°C`，When 故障出现，Then heater duty 立即归零、`heater_enabled=false`，并进入 fault-latch。
- Given fault-latch 已存在，When 故障已消失且用户再次短按中键，Then 才允许重臂；When 故障未消失，Then 重臂必须被拒绝。
- Given 温度 `>=360°C`，When 控制循环更新，Then fan 必须自动开启；Given fan 已开启，When 温度降到 `<340°C`，Then fan 才允许关闭。
- Given PD 状态发生变化，When monitor 日志输出，Then 可以看到 PD 状态更新，但 heater 不会因此被锁死或自动清臂。

## 实现前置条件（Definition of Ready / Preconditions）

- `flux-purr` 已完成 RTD 经验标定（当前按约 `3000 mV` 有效分压换算）。
- 前面板五向输入与现有 Dashboard / Menu 路由已可在真机上稳定使用。
- 板级 flash / monitor 统一通过 `mcu-agentd` 执行。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test --manifest-path firmware/Cargo.toml`
- `source /Users/ivan/export-esp.sh && cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin flux-purr --release`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard-manual docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-manual.framebuffer.bin`

### UI / Firmware Preview

- owner-facing 预览必须来自 `frontpanel_preview` 的确定性输出。
- 视觉证据必须落在本 spec 的 `./assets/` 下，并和聊天回图保持同源。

## 文档更新（Docs to Update）

- `docs/specs/README.md`
- `docs/specs/fk3u7-frontpanel-input-interaction/SPEC.md`
- `docs/specs/223uj-frontpanel-ui-contract/SPEC.md`
- `firmware/README.md`

## Visual Evidence

- Dashboard manual firmware preview（实时温度 / `SET` / 实际风扇状态 / 实际 heater 输出条）：

![Dashboard manual runtime preview](./assets/dashboard-manual.png)

- Active Cooling policy preview（正式 runtime 中改为只读安全策略说明页）：

![Active Cooling policy preview](./assets/active-cooling.png)

- 板级启动日志证据：
  - flash session: `/Users/ivan/Projects/Ivan/flux-purr/.mcu-agentd/sessions/esp32s3_frontpanel/20260421_105455.session.ndjson`
  - monitor: `/Users/ivan/Projects/Ivan/flux-purr/.mcu-agentd/monitor/esp32s3_frontpanel/20260421_105459_702.mon.ndjson`
  - 已确认默认启动 `heater_arm=false`、`heater_out=0%`、`fan=false`，并持续输出 PID/RTD defmt 日志。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 落地正式 `HeaterController`、heater fault-latch 与 overtemp fan runtime
- [x] M2: 落地 Dashboard 真实运行态字段、输入语义修正与渲染更新
- [x] M3: 补齐单测、构建与 frontpanel preview 证据
- [x] M4: 完成板级 flash/monitor 验证并收敛到 merge-ready PR

## 方案概述（Approach, high-level）

- 用单一 `HeaterController` 管理 PID、fault-latch 与 re-arm 语义，避免把控制逻辑散落在 UI reducer 和 GPIO 分支里。
- 用 Dashboard 的 `heater_output_percent` 与 `fan_enabled` 作为真实运行态展示层，不再复用 mock-only flag 表达真实执行输出。
- 继续把 CH224Q 作为电源准备层而不是 heater interlock，避免把“功率不足”误处理成“安全故障”。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：当前 PID 默认参数仍需依赖实板热惯性验证，首次实现只能先选保守固定值。
- 风险：RTD 经验标定仍是经验值，不是外部标准校准；高温绝对精度仍可能需要后续单独处理。
- 风险：如果热板本体散热过强，风扇仅超温开启的策略可能让回落速度偏慢，但不影响当前安全目标。
- 假设：当前 heater 与 fan 硬件极性已经按现有 bring-up 经验验证为正确。

## 变更记录（Change log）

- 2026-04-21: 创建正式 heater PID 闭环与前面板运行态同步 spec，冻结控制包线、fault-latch、fan hysteresis 与 PD 观测语义。
- 2026-04-21: 收敛到 merge-ready PR #11，并补齐 preview / board proof 路径。

## 参考（References）

- `../223uj-frontpanel-ui-contract/SPEC.md`
- `../fk3u7-frontpanel-input-interaction/SPEC.md`
- `../../hardware/heater-power-switch-design.md`
- `../../hardware/s3-frontpanel-baseline.md`
