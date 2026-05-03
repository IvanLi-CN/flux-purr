# Flux Purr 正式 PID 加热闭环与前面板运行态同步（#q2aw6）

## 状态

- Status: 已完成
- Created: 2026-04-21
- Last: 2026-04-25

## 背景 / 问题陈述

- 当前 `flux-purr` 已完成前面板输入、RTD 读取、CH224Q 默认电压请求与 heater/fan bring-up，但 Dashboard 的风扇语义仍残留旧的单布尔开关口径。
- `#223uj` 与 `#fk3u7` 已冻结前面板视觉和五向输入基线，但 Dashboard 的 fan line、Active Cooling 页面和过温告警仍缺少统一真相源。
- 若不把风扇策略、过温停热、feature-selected PD 默认请求与前面板显示一次收口，后续板级调试会持续混淆“策略开关”“实际输出”“保护联动”三套状态。

## 目标 / 非目标

### Goals

- 把 `GPIO47` 固定占空比加热替换为按 `target_temp_c` 驱动的正式闭环；当 CH224Q 读取到 PPS APDO 覆盖 `20V` 时，heater 后端使用 `PPS/AVS 调压 + MOS 静态通断`，否则回退原 `GPIO47` PWM 调功。
- 让 Dashboard 稳定显示实时温度、设定温度、`OFF/AUTO/RUN` 三态风扇显示与实际 heater 输出强度。
- 冻结正式风扇/保护包线：
  - heater `OFF` 且 active cooling `ON`：`40~60°C` 以 `GPIO36 duty=50%`（`500‰`）运行、`>60°C` 以 `GPIO36 duty=0%`（`0‰`）全速；一旦温度回落到 `<40°C`，继续以 `GPIO36 duty=100%`（`1000‰`）拖尾 `30s` 后再关闭。
  - heater `ON`：`<=100°C` 不主动散热，超过 `100°C` 后由安全链路接管。
  - active cooling `OFF`：`>100°C` 进入最低电压 `0.1Hz` 使能脉冲，脉冲占空比按 `floor((temp-100)/10)%` 递增并封顶 `25%`。
  - active cooling `OFF` 且 `>350°C`：锁住停热并保持风扇 `50%`；`>360°C` 改为全速。
  - `temp >= 420°C`：保持 heater hard cutoff fault-latch。
- 默认启动时把 CH224Q 请求固定为 `20V`，再读取 CH224Q `0x60~0x8F` power data；只有 PPS capability 覆盖 `20V` 时才启用可调加热后端。可调请求范围为 `12V..28V`，并受 source capability 钳制。
- `heater-fixed-pd-step-experiment` 为台架验证模式：开启后若 source 同时宣告固定 `12V` 与 `20V` PDO，固件优先使用 Fixed PD `12V/20V` 离散换档后端，即使 PPS/AVS 可用也不选 `pps-mos`。
- 产出 merge-ready 所需的 spec、视觉证据、板级验证与 review 收敛材料。

### Non-goals

- 不提供运行时 PID 参数调节入口。
- 不实现 fan tach 闭环、4 线 PWM、持久化风扇档位或默认固件按 VIN 自动切换固定 PD 请求。
- 不修改外部 HTTP / RPC / 持久化字段结构。
- 不扩展新的前面板菜单层级或联网业务逻辑。

## 范围（Scope）

### In scope

- `firmware/src/bin/flux_purr.rs`
- `firmware/src/frontpanel/**`
- `firmware/src/bin/frontpanel_preview.rs`
- `web/src/features/frontpanel-preview/**`
- `web/src/stories/FrontPanelDisplay.stories.tsx`
- `firmware/README.md`
- `docs/interfaces/http-api.md`
- `docs/specs/q2aw6-heater-pid-frontpanel-runtime/**`
- `docs/specs/fk3u7-frontpanel-input-interaction/SPEC.md`
- `docs/specs/223uj-frontpanel-ui-contract/SPEC.md`

### Out of scope

- Web 控制台、HTTP API、Wi‑Fi 配置写回字段扩展
- 多电压 / 多功率档位与自动 PD 策略切换
- RTD 额外校准界面或外部校准协议

## 需求（Requirements）

### MUST

- heater 控制周期固定为 `1 Hz`。`pps-mos` 后端下 `GPIO47` 只允许静态 `0% / 100%` 输出，中间功率由 `12V..28V` 可调 PD 请求承担；fallback 后端继续使用 `2 kHz` PWM。
- 目标温度与 preset 写入都必须 clamp 到 `0~400°C`。
- RTD 开路、短路、ADC 读失败、`temp >= 420°C` 时，heater 必须立即关断并进入 fault-latch。
- fault-latch 期间 heater 不得自动恢复；故障解除后必须由用户再次短按中键重臂。
- CH224Q 在启动时默认请求 `20V`；`pd-request-12v` / `pd-request-28v` 仅改变默认固定请求值。随后必须读取 CH224Q power data 并只在 PPS APDO 覆盖 `20V` 时启用 `pps-mos`。固定 `20V` PDO 不得被当作 PPS 覆盖 `20V`。
- `pps-mos` 后端中，控制输出 `0%` 必须关 MOS，并请求 `12V` 或 source 宣告的更高 PPS 最小电压；控制输出 `1..100%` 必须映射到 `12V..28V` 并受 source capability 上下限钳制，先关 MOS、写入 PPS/AVS 电压、settle 后再开 MOS。任一关键调压写入失败必须切回固定 PD + `GPIO47` PWM fallback。
- `heater-fixed-pd-step-experiment` 开启时，CH224Q power data 必须同时包含固定 `12V` 与 `20V` PDO 才能进入 `fixed-pd-step-mos`；该后端冷态与 `0%` 输出请求 `12V`，温度 `>=80°C` 且控制输出非零时请求 `20V`，温度 `<=70°C` 或输出归零时回到 `12V`。
- `fixed-pd-step-mos` 每次换档必须先关 MOS，写 CH224Q `0x0A` 固定电压档，等待 settle，再通过 `GPIO1` VIN 分压采样确认目标电压；缺少固定 PDO、写入失败或 VIN 未确认时必须降级到 `fixed-pd-pwm-fallback`。该实验模式只提供台架验证固件，不保证所有 PD source 在 Fixed PDO 切换期间都保持 MCU rail 不掉电。
- `active_cooling_enabled=true` 时，Dashboard fan line 必须只显示 `AUTO` 或 `RUN`；`active_cooling_enabled=false` 时必须显示 `OFF`，即使保护链路正在临时驱动真实风扇。
- Dashboard 中键短按只切 heater arm；中键双击切换主动降温（`active_cooling_enabled`）；中键长按只进菜单。
- `GPIO48` 蜂鸣器必须使用独立 PWM 通道；boot 和 idle 保持静音，不得复用 heater/fan 已占用的 PWM 输出。
- heater 成功切换必须播放 `heater_on / heater_off`；主动降温成功切换必须播放 `active_cooling_on / active_cooling_off`；heater 重臂被拒绝时必须播放 `heater_reject`。
- 任何已接受的前面板用户操作都必须有提示音；其中非 heater / 主动降温专用反馈的已接受操作（如菜单导航、子页进入/退出、预设编辑）统一播放通用 `ui_input` 提示音。
- 同一个蜂鸣器 cue 被重复触发时，必须从第一拍重新开始，不得沿用上一轮尚未结束的频率段。
- 过温保护不得占用 Dashboard 的风扇元素；SET 行必须在告警激活时以 `1Hz` 闪烁 `WARN / OTEMP` 两关键帧。
- `Active Cooling` 页面在正式 runtime 中为只读安全策略说明页；用户开启这一项时，口径统一称为“开启主动降温”，并必须同步默认 `20V`（及 `12V / 28V` build variants）、`40~60°C => 50% PWM`、`>60°C => 0% PWM`、`<40°C => 100% PWM + 30s` 与 `>100 / >350 / >360°C` 包线。
- 当前风扇硬件为反相 `FB` 注入控制：`GPIO36 duty=0%` 表示最高风扇轨电压，`GPIO36 duty=100%`（`1000‰`）才表示最低风扇轨电压；所有 `minimum-voltage profile` 语义都必须落到该 `1000‰` 档位。
- 任一活动保护（`SensorShort / SensorOpen / AdcReadFailed / OverTemp`）出现时，蜂鸣器必须立即进入急促、持续的循环警告音；保护解除后改为每 `10s` 一次 reminder，直到用户任意输入确认。
- defmt 日志必须覆盖 RTD 读数、PID 输入/输出、heater backend 选择、PPS/AVS 请求电压、MOS gate 输出、fault 原因、fan policy 输出与 PD 状态变化。

### SHOULD

- cooling-disabled lock 的标签与恢复路径保持稳定，便于 monitor 与后续 review 收敛。
- 初始 UI 应在第一次有效 RTD 样本后就显示实际温度，而不是长时间保留 bring-up 默认值。
- firmware preview 与 Storybook 的 Dashboard/Active Cooling 文案和颜色层级保持一致。

### COULD

- 后续在同一条 PID 日志上扩展功率估算或 duty limit 观察字段。

## 功能与行为规格（Functional / Behavior Spec）

### Core flows

- 启动后先请求 feature-selected 固定 PD 电压（默认 `20V`），随后读取 CH224Q status 与 power data。若 PPS APDO 覆盖 `20V`，heater 后端进入 `pps-mos`；否则进入 `fixed-pd-pwm-fallback`。
- 若构建启用 `heater-fixed-pd-step-experiment` 且 CH224Q power data 宣告固定 `12V` 与 `20V` PDO，heater 后端进入 `fixed-pd-step-mos` 并强制优先于 `pps-mos`；否则保持正式运行时的 PPS/fallback 选择规则。
- 用户短按中键后，heater 进入 arm 状态；若无 fault-latch，则控制器按 `target_temp_c - current_temp_c` 输出 `0..100%` 控制量。`pps-mos` 后端把该控制量映射到可调 PD 电压并静态打开 MOS；fallback 后端把该控制量作为原 PWM duty。
- Dashboard 上/下短按和 hold-repeat 都只调整 `target_temp_c`，每次事件步进 `1°C` 并继续 clamp 到 `0~400°C`；中键 heater / active cooling / menu 语义不受 hold-repeat 影响。
- 用户双击中键后，切换的是“主动降温”策略位，而不是直接强制 fan GPIO。
- Dashboard fan line 只反映“策略开关 + 当前是否实际运行”：
  - `OFF`：风扇策略关闭
  - `AUTO`：风扇策略开启但当前无需工作
  - `RUN`：风扇策略开启且当前已使能输出
- 当 `active_cooling_enabled=true` 且温度位于 `40~60°C` 时，真实风扇必须使用 `GPIO36 duty=50%`（`500‰`）；当温度 `>60°C` 时必须切到 `GPIO36 duty=0%`（`0‰`）全速；当温度从 `>=40°C` 回落到 `<40°C` 时，真实风扇必须继续以 `GPIO36 duty=100%`（`1000‰`）运行 `30s`，然后才关闭。
- 当 `active_cooling_enabled=false` 且 `temp > 350°C` 时，heater 必须被强制关断并锁住；用户重新开启风扇策略或手动重新使能 heater 后才允许退出该锁态。
- 当 `active_cooling_enabled=false` 且 `temp > 360°C` 时，真实风扇输出升级为全速，但 Dashboard fan line 仍保持 `OFF`。
- PD 状态只做观测：即使 PD 丢失或降档，也不自动清空 `heater_enabled`。但 PPS/AVS 调压写入失败会把 heater 后端降级到固定 PD PWM fallback。
- 任一活动保护出现时，蜂鸣器立即切到持续 alarm；fault clear 后停止连续 alarm，并改为每 `10s` 的 reminder cadence，直到任意输入确认。

### Edge cases / errors

- 首次 RTD 采样失败时，heater 必须保持关断，直到后续有效样本恢复且用户重新 arm。
- fault-latch 期间若用户再次短按中键：
  - 当前 fault 仍存在时，必须拒绝重臂并保持 `heater_enabled=false`
  - 当前 fault 已消失时，允许清除 latch 并重新进入 arm
- cooling-disabled lock 清除后，若温度仍高于 `350°C`，必须等待温度回到 `<=350°C` 再次越线后才允许重新触发锁定。
- reminder pending 期间，第一次任意输入只能作为确认/静音；该输入不得顺带切 heater、切主动降温或发生页面导航。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `FrontPanelUiState.fan_display_state` | Rust state model | internal | New | None | firmware | runtime / preview / render tests | Dashboard 风扇三态真相源 |
| `FrontPanelUiState.heater_lock_reason` | Rust state model | internal | New | None | firmware | runtime / preview / render tests | `cooling-disabled-overtemp` / `hard-overtemp` |
| `FrontPanelUiState.dashboard_warning_visible` | Rust state model | internal | New | None | firmware | runtime / preview / render tests | SET 行告警闪烁相位 |
| `FrontPanelRuntimeState` / `FrontPanelScreen` | TypeScript type | internal | Updated | None | web | Storybook / preview harness | 对齐 firmware 三态 fan 与告警关键帧 |

### 契约文档（按 Kind 拆分）

None

## 验收标准（Acceptance Criteria）

- Given 固件刚启动，When RTD 已有有效样本，Then Dashboard 左侧显示实时温度，右侧显示 `SET/PPS/FAN`，其中 `FAN` 只会显示 `OFF/AUTO/RUN`。
- Given Dashboard，When 用户短按中键，Then 只切换 heater arm；When 双击中键，Then 只切换主动降温；When 长按中键，Then 仍进入菜单；When 长按保持上/下，Then 只连续调整 `target_temp_c`。
- Given heater 关闭且主动降温开启，When 温度 `39°C / 40°C / 60°C / 61°C`，Then fan 必须分别进入停止或 30 秒拖尾 / `50% PWM` / `50% PWM` / `0% PWM`。
- Given 主动降温已经把风扇拉起，When 温度跌到 `<40°C`，Then fan 必须以 `100% PWM` 再持续 `30s` 后关闭。
- Given heater 开启，When 温度 `<=100°C`，Then fan 不得因为 idle cooling 阈值而提前启动。
- Given active cooling 关闭，When 温度 `100 / 110 / 350 / 351 / 361°C`，Then fan 必须分别满足无脉冲 / `1%` 脉冲 / `25%` 脉冲 / `50%` / 全速。
- Given active cooling 关闭且温度 `>350°C`，When 控制循环更新，Then heater 必须被锁住停热；When 用户重新开启风扇策略或手动重新 arm heater，Then 才允许离开锁态。
- Given `temp >= 420°C`，When 故障出现，Then heater 立即归零并进入 `hard-overtemp` fault-latch。
- Given Dashboard 过温告警，When 页面刷新，Then 告警只占据 SET 行并以两关键帧闪烁，FAN 行不切换到告警文案。
- Given CH224Q power data 包含覆盖 `20V` 的 PPS APDO，When runtime 初始化 heater 后端，Then 选择 `pps-mos`，`0% / 50% / 100%` 控制量分别请求 `12V / 20V / 28V`（若 source capability 允许）且 GPIO47 只输出静态关/开。
- Given CH224Q 只提供固定 `20V` PDO 或 PPS APDO 不覆盖 `20V`，When runtime 初始化 heater 后端，Then 选择 `fixed-pd-pwm-fallback`，不得把固定 `20V` 误判为 PPS 可调能力。
- Given 构建启用 `heater-fixed-pd-step-experiment` 且 CH224Q power data 同时包含固定 `12V` 与 `20V` PDO，When runtime 初始化 heater 后端，Then 选择 `fixed-pd-step-mos` 并强制优先于 PPS/AVS；When 温度从冷态升至 `>=80°C` 且 heater 输出非零，Then 切到 `20V`；When 温度降到 `<=70°C` 或 heater 输出为 `0%`，Then 回到 `12V`。
- Given `fixed-pd-step-mos` 正在换档，When CH224Q 写入失败或 `GPIO1` VIN 确认不在目标电压容差内，Then 关 MOS 并降级到 `fixed-pd-pwm-fallback`。

## 实现前置条件（Definition of Ready / Preconditions）

- `flux-purr` 已完成 RTD 经验标定（当前按约 `3000 mV` 有效分压换算）。
- 前面板五向输入与现有 Dashboard / Menu 路由已可在真机上稳定使用。
- 板级 flash / monitor 统一通过 `mcu-agentd` 执行。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test --manifest-path firmware/Cargo.toml`
- `source /Users/ivan/export-esp.sh && cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin flux-purr --release`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-pps-12v.framebuffer.bin --pd-mv 12000`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-pps-28v.framebuffer.bin --pd-mv 28000`
- `bun run --cwd web check`
- `bun run --cwd web typecheck`
- `bun run --cwd web build-storybook`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard-fan-off docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-fan-off.framebuffer.bin`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard-fan-auto docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-fan-auto.framebuffer.bin`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard-fan-run docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-fan-run.framebuffer.bin`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard-overtemp-a docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-overtemp-a.framebuffer.bin`
- `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard-overtemp-b docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard-overtemp-b.framebuffer.bin`

### UI / Firmware Preview

- owner-facing 预览必须来自 `frontpanel_preview` 或 Storybook 的确定性输出。
- 视觉证据必须落在本 spec 的 `./assets/` 下，并和聊天回图保持同源。

## 文档更新（Docs to Update）

- `docs/specs/fk3u7-frontpanel-input-interaction/SPEC.md`
- `docs/specs/223uj-frontpanel-ui-contract/SPEC.md`
- `firmware/README.md`
- `docs/interfaces/http-api.md`

## Visual Evidence

- Dashboard fan `OFF`：

![Dashboard fan off](./assets/dashboard-fan-off.png)

- Dashboard fan `AUTO`：

![Dashboard fan auto](./assets/dashboard-fan-auto.png)

- Dashboard fan `RUN`：

![Dashboard fan run](./assets/dashboard-fan-run.png)

- Dashboard PPS `12V`：

![Dashboard PPS 12V](./assets/dashboard-pps-12v.png)

- Dashboard PPS `28V`：

![Dashboard PPS 28V](./assets/dashboard-pps-28v.png)

- Dashboard overtemp warning frame A：

![Dashboard overtemp A](./assets/dashboard-overtemp-a.png)

- Dashboard overtemp warning frame B：

![Dashboard overtemp B](./assets/dashboard-overtemp-b.png)

- Active Cooling policy page：

![Active Cooling](./assets/active-cooling.png)

- Current default temperature palette（Aurora / C）：

![Temperature palette current](./assets/temperature-palette-current.png)

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 落地正式 `HeaterController`、heater fault-latch 与风扇策略状态机
- [x] M2: 落地 Dashboard 三态风扇显示、SET 行告警闪烁与 Active Cooling 只读说明页
- [x] M3: 补齐单测、host preview、Storybook 故事与视觉证据
- [x] M4: 完成 xtensa build / review 收敛并准备 merge-ready PR

## 方案概述（Approach, high-level）

- 用单一 `HeaterController` 管理 PID 与 hard fault-latch，再把 cooling-disabled lock 作为独立安全层挂在 fan policy 旁边。
- 用 `fan_display_state + heater_lock_reason + dashboard_warning_visible` 作为 Dashboard 真相源，不再复用单布尔 fan 标记表达全部运行态。
- 用 `HeaterPowerBackend` 把控制器输出与硬件输出解耦：`pps-mos` 后端只做 MOS 静态通断并通过 CH224Q PPS/AVS 调压；`fixed-pd-pwm-fallback` 保留原 `GPIO47` PWM 调功。
- 实验 feature 在同一 `HeaterPowerBackend` 内新增 `fixed-pd-step-mos`：它只在固定 `12V/20V` PDO 都存在时启用，使用 `80°C/70°C` 滞回做离散换档，并通过 VIN ADC 作为换档确认。
- CH224Q 仍作为电源准备层而不是 heater interlock；只有启动 capability gate 与后续调压写入失败会影响 heater 后端选择。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：当前 PID 默认参数仍需依赖实板热惯性验证，首次实现只能先选保守固定值。
- 风险：RTD 经验标定仍是经验值，不是外部标准校准；高温绝对精度仍可能需要后续单独处理。
- 风险：Fixed PDO `12V/20V` 切换是否会让 VBUS 或 MCU 供电短暂跌落取决于 CH224Q、线缆、PD source 与负载瞬态，必须通过台架示波器和复位日志确认；固件只能在换档前关 MOS 并用 VIN 采样确认换档结果。
- 风险：`0.1Hz` 风扇脉冲与半速 / 全速切换基于当前板级风扇 rail 映射，后续若硬件变更需重新验证。
- 假设：当前 heater 与 fan 硬件极性已经按现有 bring-up 经验验证为正确。

## 参考（References）

- `../223uj-frontpanel-ui-contract/SPEC.md`
- `../fk3u7-frontpanel-input-interaction/SPEC.md`
- `../../hardware/heater-power-switch-design.md`
- `../../hardware/s3-frontpanel-baseline.md`
