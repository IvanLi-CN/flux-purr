# Flux Purr C3 + CH224Q/CH442E 前面板基线落地（#233y7）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-03

## 背景 / 问题陈述

- `flux-purr` 当前固件与接口仍是初始化骨架，硬件口径默认 `ESP32-S3`，与本次 C3 二开目标不一致。
- 当前缺少针对 `ESP32-C3-FH4 + CH224Q + CH442E + TCA6408A 前面板` 的统一 GPIO 与接口契约，后续实现容易出现脚位冲突与状态字段漂移。
- 若不先冻结规格并同步实现，固件、Web 控制台、接口文档会持续分叉，导致联调成本上升。

## 目标 / 非目标

### Goals

- 冻结并落地 C3 方案的 GPIO 分配（总数严格 15，零余量）。
- 在 firmware 中实现可编译的适配层：CH224Q、CH442E、TCA6408A 前面板映射。
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

- GPIO 锁定为以下 15 路且无重复：`0,1,2,3,4,5,6,7,8,9,10,18,19,20,21`。
- CH224Q 适配层支持 `0x22` 与 `0x23` 地址识别，支持 `5/9/12/15/20/28V` 控制寄存器编码。
- CH442E 适配层支持 `IN/EN#` 组合映射 `Mcu/Sink/Disabled`。
- TCA6408A 前面板支持 `P0~P4` 五向键解码，`P5/P6` 预留 LCD `RES/CS`。
- `DeviceStatus` 新增字段：`pd_request_mv`、`pd_contract_mv`、`usb_route`、`fan_enabled`、`fan_pwm_permille`、`frontpanel_key`。
- 新增枚举：`UsbRoute`（`Mcu|Sink`）、`PdState`（`Negotiating|Ready|Fallback5V|Fault`）。
- HTTP 与 Web 字段同步，最小展示包含 PD 档位、USB 路由、风扇状态。

### SHOULD

- CH442E 上电先进入安全态，再初始化到默认 MCU 路由。
- 状态字段保持向后兼容（加法变更，不移除既有字段语义）。

### COULD

- 为后续实机联调预留更多状态诊断位（例如 CH224 协议位图透传）。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 固件启动时加载 C3 board profile，构建固定 GPIO 表并完成预算一致性校验。
- CH224Q 通过 I2C 地址识别与寄存器编码生成可写入控制字。
- CH442E 根据 `IN` 与 `EN#` 计算有效路由，默认初始化为 `Mcu`。
- 前面板扩展器输入寄存器被解码为单一方向键事件，歧义输入返回空。
- 设备状态快照被 HTTP/Web 使用，展示新增 PD/路由/风扇字段。

### Edge cases / errors

- I2C 地址不在 `0x22/0x23` 时必须返回错误。
- CH442E 在 `EN#` 未使能时必须报告 `Disabled`。
- 前面板同时多键按下时不输出方向键事件，避免歧义。
- GPIO 预算一旦重复或非 15，单测必须失败。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Device REST API | HTTP | external | Modify | ../../interfaces/http-api.md | firmware + web | web console | 状态字段增加，保持加法兼容 |
| Device status model | Rust type | internal | Modify | /firmware/src/lib.rs | firmware | firmware + web mock | 新增 PD/路由/风扇/前面板字段 |

### 契约文档（按 Kind 拆分）

- `../../interfaces/http-api.md`

## 验收标准（Acceptance Criteria）

- Given C3 board profile 已落地，When 运行 `gpio_budget_is_exactly_15`，Then 测试通过且 GPIO 总数为 15 且不重复。
- Given CH224Q 适配层，When 对 `0x22/0x23` 进行解析并编码 `5/9/12/15/20/28V`，Then 地址解析与寄存器编码结果正确。
- Given CH442E 适配层，When 输入 `IN/EN#` 组合，Then 能得到 `Mcu/Sink/Disabled` 且默认初始化后为 `Mcu`。
- Given TCA6408A 前面板输入，When 解析 `P0~P4`，Then 五向键映射无歧义，多键返回空。
- Given 接口与 Web 已同步，When 检查 `docs/interfaces/http-api.md` 和 `web` 相关类型/mock/UI，Then 新增字段均可被展示或使用。
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
- [x] M2: 固件 C3 board profile 与 CH224Q/CH442E/TCA6408A 适配层落地
- [x] M3: HTTP/Web 字段同步并通过全量质量门禁

## 方案概述（Approach, high-level）

- 先 docs 锁口径，再实现固件适配层，最后同步接口与 Web，按原子切片提交。
- 适配层优先提供纯逻辑编码/映射能力，保证主机侧可测试与可回归。
- 通过 spec drift 检查确保实现与规格持续一致，最终在 normal-flow Step 4 收口。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：GPIO 零余量，后续新增外设将直接触发重分配。
- 风险：CH442E `EN#` 极性若实物不一致需驱动层反转。
- 开放问题：实机联调时 CH224Q 是否稳定使用 `0x22`，需保留 `0x23` 兼容路径。
- 假设：LCD `CS/RES` 持续走 TCA6408A，不回退 MCU 直连。

## 变更记录（Change log）

- 2026-03-03: 创建规格并冻结 C3 二开的 GPIO、器件与接口同步范围。
- 2026-03-03: 完成硬件口径文档与 specs 索引登记，里程碑更新为 1/3。
- 2026-03-03: 完成 firmware 适配层与状态模型扩展，里程碑更新为 2/3。
- 2026-03-03: 完成 HTTP/Web 同步与全量检查，规格状态更新为已完成。

## 参考（References）

- https://documentation.espressif.com/esp32-c3_datasheet_en.html
- https://file.wch.cn/download/file?id=301
- /Users/ivan/Projects/Ivan/iso-usb-hub/docs/hardware/front_panel_netlist.enet.enet
- /Users/ivan/Projects/Ivan/iso-usb-hub/docs/hardware/mainboard_netlist.enet.enet
