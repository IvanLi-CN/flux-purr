# Flux Purr C3 + CH224Q 前面板基线更新（移除 CH442E，新增 VIN ADC）（#233y7）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-28

## 背景 / 问题陈述

- `flux-purr` 当前固件与接口仍是初始化骨架，硬件口径默认 `ESP32-S3`，与本次 C3 二开目标不一致。
- 当前基线仍残留 `CH442E` 路由假设，但方案已改为不再引入该芯片，同时需要 MCU 直接采样 USB PD 主输入电压，并将 `FAN_EN` 回到 MCU 直连。
- 若不先冻结规格并同步实现，固件、Web 控制台、接口文档会持续分叉，导致联调成本上升。

## 目标 / 非目标

### Goals

- 冻结并落地 C3 方案的新 GPIO 分配（`14` 路 active，保留 `GPIO9`）。
- 在 firmware 中实现可编译的适配层：CH224Q、TCA6408A 前面板映射，以及 VIN 采样板级常量。
- 扩展设备状态模型并同步 HTTP 契约与 Web 控制台字段。
- 按 `normal-flow` 收口至本地 PR-ready（不 push、不建 PR）。

### Non-goals

- 不进行板级烧录、示波器验证或真实 PD 适配器联调。
- 不引入计划外器件替代路线。
- 不进入 fast-track 远端 PR 流程。

## 范围（Scope）

### In scope

- `docs/specs/233y7-c3-ch224q-ch442e-frontpanel/SPEC.md` 与索引更新。
- 新增/更新硬件设计文档（GPIO、电源链路、前面板映射、strapping 约束）。
- `firmware/` 内 board profile、适配层逻辑、单元测试、状态模型更新。
- `docs/interfaces/http-api.md` 与 `web/src/features/device-console/*` 对齐更新。

### Out of scope

- 真实硬件驱动寄存器全量实现（仅落地可编译适配层与编码逻辑）。
- 增加风扇测速回路（`fan_tach` 已明确不接 MCU）。
- 引入新 CI 工具或改写现有质量门禁脚本。

## 需求（Requirements）

### MUST

- Active MCU GPIO 锁定为以下 14 路且无重复：`0,1,2,3,4,5,6,7,8,10,18,19,20,21`。
- `GPIO8` 直连 `FAN_EN`，`GPIO9` 保留为 strapping-sensitive spare。
- CH224Q 适配层支持 `0x22` 与 `0x23` 地址识别，支持 `5/9/12/15/20/28V` 控制寄存器编码。
- VIN sense 使用 `GPIO1 / ADC1_CH1`，分压标称值 `56 kOhm / 5.1 kOhm`，覆盖 `28V` 及以下输入。
- TCA6408A 前面板支持 `P0~P4` 五向键解码，`P5/P6` 预留 LCD `RES/CS`，`P7` 保留。
- `DeviceStatus` 维护字段：`pd_request_mv`、`pd_contract_mv`、`fan_enabled`、`fan_pwm_permille`、`frontpanel_key`。
- 保留枚举：`PdState`（`Negotiating|Ready|Fallback5V|Fault`）。
- HTTP 与 Web 字段同步，最小展示包含 PD 档位、输入电压、风扇状态。

### SHOULD

- VIN sense 节点使用 `1%` 电阻，板级预留 `100 nF` 小电容抑制采样抖动。
- `FAN_EN` 所在 `GPIO8` 在复位窗口必须保持低电平，避免影响 boot strap；建议加弱下拉。
- 对外状态字段与当前板级器件保持一致，不继续暴露已移除器件的占位字段。

### COULD

- 为后续实机联调预留更多状态诊断位（例如 ADC 原始计数或校准状态透传）。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 固件启动时加载 C3 board profile，构建固定 GPIO 表并完成预算一致性校验。
- CH224Q 通过 I2C 地址识别与寄存器编码生成可写入控制字。
- 前面板扩展器输入寄存器被解码为单一方向键事件，歧义输入返回空。
- 设备状态快照被 HTTP/Web 使用，展示新增 PD/输入电压/风扇字段。

### Edge cases / errors

- I2C 地址不在 `0x22/0x23` 时必须返回错误。
- 前面板同时多键按下时不输出方向键事件，避免歧义。
- GPIO active/reserved 映射一旦重复或总数不是 `14 + 1`，单测必须失败。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Device REST API | HTTP | external | Modify | ../../interfaces/http-api.md | firmware + web | web console | 删除 `usbRoute`，补充 VIN sense 说明 |
| Device status model | Rust type | internal | Modify | /firmware/src/lib.rs | firmware | firmware + web mock | 对齐 PD/输入电压/风扇/前面板字段 |

### 契约文档（按 Kind 拆分）

- `../../interfaces/http-api.md`

## 验收标准（Acceptance Criteria）

- Given C3 board profile 已落地，When 运行 `gpio_map_is_valid`，Then 测试通过且 active/reserved GPIO 总数为 `14 + 1` 且不重复。
- Given CH224Q 适配层，When 对 `0x22/0x23` 进行解析并编码 `5/9/12/15/20/28V`，Then 地址解析与寄存器编码结果正确。
- Given VIN sense 方案，When 按 `56 kOhm / 5.1 kOhm` 计算 `28V` 输入，Then ADC 引脚电压不高于 `2.337V`。
- Given TCA6408A 前面板输入，When 解析 `P0~P4`，Then 五向键映射无歧义，多键返回空。
- Given 接口与 Web 已同步，When 检查 `docs/interfaces/http-api.md` 和 `web` 相关类型/mock/UI，Then 已移除 `usbRoute` 且输入电压字段仍可被展示或使用。
- Given 本轮改动，When 运行仓库既有 checks，Then 结果明确且全部通过。

## 实现前置条件（Definition of Ready / Preconditions）

- 方案器件与 GPIO 分配已由主人确认。
- 交付流程锁定为 `normal-flow`。
- 规格根目录使用 `docs/specs/`（已满足）。
- 本规格中的接口变更与测试门槛已冻结。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `cargo test --manifest-path firmware/Cargo.toml`
- Integration tests: N/A（本轮为主机侧逻辑适配）
- E2E tests (if applicable): `SKIP_E2E=1 bash scripts/check-web-e2e.sh`

### UI / Storybook (if applicable)

- Stories to add/update: none（复用既有 Storybook 结构）
- Visual regression baseline changes: none

### Quality checks

- `bash scripts/check-firmware-fmt.sh`
- `bash scripts/check-firmware-clippy.sh`
- `bash scripts/check-firmware-build.sh`
- `bash scripts/check-web-check.sh`
- `bash scripts/check-web-build.sh`
- `bash scripts/check-storybook-build.sh`

## 文档更新（Docs to Update）

- `docs/specs/README.md`: 追加规格索引并更新状态。
- `docs/interfaces/http-api.md`: 对齐 C3 状态字段与示例。
- `docs/hardware/c3-frontpanel-baseline.md`: 锁定 GPIO/电源树/前面板映射/strapping 约束。
- `firmware/README.md`: 默认 target 与 C3 方案说明。

## 计划资产（Plan assets）

- None

## 资产晋升（Asset promotion）

- None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 规格与硬件口径文档落地并索引登记
- [x] M2: 固件 C3 board profile 与 CH224Q/TCA6408A/VIN sense/FAN_EN 基线落地
- [x] M3: HTTP/Web 字段同步并通过全量质量门禁

## 方案概述（Approach, high-level）

- 先 docs 锁口径，再实现固件适配层，最后同步接口与 Web，按原子切片提交。
- 适配层优先提供纯逻辑编码/映射能力，保证主机侧可测试与可回归。
- 通过 spec drift 检查确保实现与规格持续一致，最终在 normal-flow Step 4 收口。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：`GPIO8` 现在承担 `FAN_EN`，其默认电平必须严格受控，否则会引入 boot strap 或上电误使能风险。
- 风险：VIN sense 精度会受 ADC 校准、输入纹波与分压电阻误差影响。
- 开放问题：实机联调时 CH224Q 是否稳定使用 `0x22`，需保留 `0x23` 兼容路径。
- 假设：`FAN_EN` 直连 `GPIO8`，且板级通过弱下拉保证默认关闭。
- 假设：LCD `CS/RES` 持续走 TCA6408A，不回退 MCU 直连。

## 变更记录（Change log）

- 2026-03-03: 创建规格并冻结 C3 二开的 GPIO、器件与接口同步范围。
- 2026-03-03: 完成硬件口径文档与 specs 索引登记，里程碑更新为 1/3。
- 2026-03-03: 完成 firmware 适配层与状态模型扩展，里程碑更新为 2/3。
- 2026-03-03: 完成 HTTP/Web 同步与全量检查，规格状态更新为已完成。
- 2026-03-27: 移除 CH442E 基线，新增 GPIO1 VIN_ADC 与 `56 kOhm / 5.1 kOhm` 分压方案，清理 `usbRoute` 契约。
- 2026-03-28: 将 `FAN_EN` 从 `TCA6408A P7` 调整为 MCU `GPIO8` 直连，并保留 `GPIO9` 作为唯一未分配 strapping pin。

## 参考（References）

- https://documentation.espressif.com/esp32-c3_datasheet_en.html
- https://file.wch.cn/download/file?id=301
- /Users/ivan/Projects/Ivan/iso-usb-hub/docs/hardware/front_panel_netlist.enet.enet
- /Users/ivan/Projects/Ivan/iso-usb-hub/docs/hardware/mainboard_netlist.enet.enet
