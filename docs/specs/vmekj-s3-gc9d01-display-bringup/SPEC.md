# Flux Purr S3 GC9D01 异步 SPI 显示 bring-up 与启动后界面轮播（#vmekj）

## 状态

- Status: 已完成
- Created: 2026-04-10
- Last: 2026-04-11

## 背景 / 问题陈述

- 当前仓库的 `ESP32-S3` 固件入口只覆盖风扇 bring-up，LCD 相关引脚虽已冻结，但没有可烧录的显示驱动实现。
- 主人已经明确要求使用 `gc9d01-rs` 作为显示驱动，并且硬约束必须走 **异步 SPI**，不能用 blocking SPI 兜底。
- 若没有统一的显示测试界面、host 侧可控预览图，以及上板后用于拍照校验方向/颜色的流程，就无法可靠完成面板方向、偏移和颜色口径的闭环。
- 本轮测试通过后，设备需要在启动校准结束后持续轮播一组已冻结的前面板界面，用于主人直接在硬件上检查最终 UI 观感。

## 目标 / 非目标

### Goals

- 复用 `esp32s3-fan-cycle` binary 名称与 `mcu-agentd` artifact 路径，改造成 `ESP32-S3` 的 GC9D01 显示 bring-up 入口。
- 接入 `gc9d01-rs` async driver，使用 `Embassy + esp-hal-embassy + SPI2.into_async()` 完成面板初始化与刷屏。
- 新增一套共享显示场景：静态校准屏 + 启动后的前面板界面轮播，并完成最终收口为界面轮播常驻。
- 新增 host-side preview harness，复用同一套渲染代码导出 `framebuffer.bin` 与 `preview.png`。
- 保持 host 侧质量门和 Xtensa 构建口径可运行，并通过 `mcu-agentd` 完成烧录/监看流程。

### Non-goals

- 不扩展风扇策略范围；只保持已冻结的 `fan-cycle` 行为继续运行，不在本规格内新增 tach 闭环或新的风扇控制算法。
- 不接入触摸、按键驱动的真实菜单状态机、动画切换或实时业务状态绑定。
- 不修改 Web 控制台、HTTP API、CH224Q / heater / RTD 等其它硬件驱动。
- 不在本轮实现自动持久化“校准已完成”状态；最终启动后轮播行为仍由固件常量/代码收口。

## 范围（Scope）

### In scope

- `docs/specs/vmekj-s3-gc9d01-display-bringup/SPEC.md` 与 `docs/specs/README.md`
- `firmware/Cargo.toml` 的显示/异步运行时依赖
- `firmware/src/lib.rs` 与新增的显示模块
- `firmware/src/bin/esp32s3_fan_cycle.rs`
- host preview binary / framebuffer 导出逻辑
- `firmware/README.md`、必要的根 README 口径同步
- `docs/specs/vmekj-s3-gc9d01-display-bringup/assets/` 下的视觉证据

### Out of scope

- 量产级 UI 设计冻结
- 真实业务数据绑定
- 方向校准后的二次硬件改线或额外外设支持

## 需求（Requirements）

### MUST

- 显示驱动固定使用 `gc9d01-rs`，并锁定为 async API。
- 设备端 SPI 必须通过 `SPI2.into_async()` 进入异步模式。
- 板级显示引脚固定为：`DC=GPIO10`、`MOSI=GPIO11`、`SCLK=GPIO12`、`BLK=GPIO13`、`RES=GPIO14`、`CS=GPIO15`。
- 首轮面板 profile 按 `panel_160x50`、`width=160`、`height=50`、`dx=15`、`dy=0`、初始 `Orientation::Landscape` 实现。
- 静态校准屏必须至少包含：方向/边缘标识、彩色块、灰阶块、面板/分辨率文字。
- bring-up 阶段必须支持：`静态校准屏 -> 界面轮播`。
- 最终收口版本必须默认采用“上电后进入前面板界面轮播”。
- 最终收口版本不得破坏已冻结的风扇循环逻辑；显示轮播固件运行时仍需继续驱动 `GPIO35/36` 对应的 fan-cycle。
- host preview 必须复用同一套场景渲染代码，并产出 `framebuffer.bin` 与 `preview.png`。
- 上板方向/颜色验收必须以主人的实拍照片为最终真相源；若有偏差，只允许在同一实现范围内微调 orientation / offset / 颜色口径。

### SHOULD

- demo 序列尽量复用上游 `gc9d01-rs` embedded-graphics 示例里的典型图案（纯色、棋盘格、形状、文字、网格等）。
- host preview 输出应默认落到 spec 资产目录，便于在 `SPEC.md` 中作为视觉证据引用。
- 设备端日志应输出当前场景、方向配置与 profile 口径，便于 monitor 时定位问题。

### COULD

- 后续把静态启动屏扩展成更正式的 boot splash，只要仍兼容当前 `160x50` 面板口径。

## 功能与行为规格（Functional / Behavior Spec）

### Core flows

- 设备上电后初始化 Embassy 定时器、异步 SPI、GC9D01 driver 与背光控制。
- bring-up 阶段固件先绘制静态校准屏并显示，随后按固定顺序轮播前面板最终界面集合。
- 最终收口固件上电后先显示静态校准屏，再进入界面轮播并持续循环，等待下一次刷写或代码切换。
- host preview binary 使用与设备端相同的场景渲染入口生成 framebuffer dump，再转换成 PNG 供主人预审。
- 硬件调试阶段，主人拍摄静态校准屏和界面轮播画面；Agent 根据实拍判断是否需要微调 orientation / dx / dy / 颜色设置。
- 风扇控制与显示流程并行存在：显示轮播运行时，`fan-cycle` 继续按原冻结方案切换 `High -> Low -> Mid -> Stop`。

### Edge cases / errors

- 若异步 SPI / Embassy 初始化失败，固件应在日志中暴露初始化阶段与具体环节，而不是静默卡死。
- 若 host preview 生成的 PNG 与设备实拍不一致，优先检查驱动 orientation / address window / offset，而不是在 PNG 转换阶段做掩盖式旋转。
- 若 `mcu-agentd` 无 selector、无设备、资源忙或 artifact 缺失，必须按 `mcu-agentd` 机器人模式错误口径中止并回报证据。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `esp32s3-fan-cycle` firmware binary | CLI/binary | internal | Modify | None | firmware | mcu-agentd / bring-up operator | 路径保持不变，职责改为显示 bring-up |
| Display scene render helpers | Rust module | internal | New | None | firmware | device binary / host preview | 复用同源渲染逻辑 |
| Host display preview binary | CLI/binary | internal | New | None | firmware | developer | 导出 framebuffer 与预览图 |

### 契约文档（按 Kind 拆分）

None

## 验收标准（Acceptance Criteria）

- Given host preview harness，When 生成启动屏 framebuffer 与 PNG，Then 预览图能显示方向标识、RGB 色块、灰阶块和文字标签。
- Given device binary，When 使用 Xtensa 目标构建，Then `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin esp32s3-fan-cycle --release` 成功。
- Given host 质量门，When 运行 `cargo test`、`cargo clippy --all-targets --all-features -D warnings`、`cargo build --release`，Then 全部通过。
- Given bring-up 验证版固件，When 固件启动，Then 能先显示静态校准屏，再循环播放前面板最终界面。
- Given 主人提供实拍照片，When 对比 host preview 与实机效果，Then 能明确确认或修正方向、镜像、偏移与 RGB/灰阶口径。
- Given 最终收口版本，When 设备再次上电，Then 在后续未替换固件前默认进入前面板界面轮播。
- Given 最终收口版本，When 设备保持运行，Then 风扇继续按冻结的 `fan-cycle` 节奏工作，不因显示收口而退化为闲置。

## 实现前置条件（Definition of Ready / Preconditions）

- `gc9d01-rs` upstream async API 与 `panel_160x50` profile 可用。
- 仓库现有 `ESP32-S3` Xtensa 构建口径可用。
- 主人已冻结异步 SPI、板级引脚与最终静态启动屏目标。
- `mcu-agentd.toml` 继续沿用 `esp32s3_frontpanel` 目标与当前 artifact 路径。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `cargo test --manifest-path firmware/Cargo.toml`
- Lint: `cargo clippy --manifest-path firmware/Cargo.toml --all-targets --all-features -- -D warnings`
- Host build: `cargo build --manifest-path firmware/Cargo.toml --release`
- Xtensa build: `source /Users/ivan/export-esp.sh && cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin esp32s3-fan-cycle --release`

### Quality checks

- host preview 与设备预览使用同一渲染源
- 视觉证据在本 spec 的 `## Visual Evidence` 中留档
- 若进入 PR 路径，必须完成 spec sync 与 review proof

## 文档更新（Docs to Update）

- `docs/specs/README.md`
- `firmware/README.md`
- 如有必要，`README.md`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新建显示 bring-up spec 与索引
- [x] M2: 落地 async SPI + GC9D01 设备端入口
- [x] M3: 落地共享显示场景与 host preview harness
- [x] M4: 完成 host/Xtensa 验证、视觉证据与硬件烧录口径

## 方案概述（Approach, high-level）

- 场景渲染采用独立的逻辑 framebuffer/canvas，让 host preview 与 device binary 共享同一套绘制代码。
- 设备端只负责初始化 `gc9d01-rs` driver，并把共享 canvas 内容拷入驱动 framebuffer 再 flush 到屏幕。
- host preview 默认导出 owner-facing 的逻辑 framebuffer（`RGB565 LE`，`160x50`）用于 PNG 预览，同时额外导出与 GC9D01 设备端一致的 panel-order companion framebuffer（已应用 orientation 变换，`RGB565 BE`，`50x160`），避免另写一套 UI 逻辑。
- 通过单一常量控制“仅校准屏”、“demo 测试轮播”和“前面板界面轮播”三种启动模式的切换。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：上游 `gc9d01-rs` 的 `Orientation::Landscape` 与当前板子的实际装配方向可能不一致，仍需实拍确认。
- 风险：`panel_160x50` 的 `dx=15` 与实际模组可视区若存在偏差，需要在同一轮实现里微调。
- 风险：host preview 反映的是逻辑场景与 panel-order companion 的组合证据，不等于实机面板玻璃、背光、色温与拍照白平衡的真实观感。
- 风险：若 `mcu-agentd` selector 未设置或串口被占用，会阻断上板验证。
- 假设：当前前面板模组确实与 `gc9d01-rs` 的 `panel_160x50` 配置兼容。
- 假设：最终启动后界面轮播无需在 EEPROM/NVS 中持久化状态，只需固件代码冻结默认行为。

## Visual Evidence

- Host startup preview（逻辑预览，`RGB565 LE`，`160x50`，`Orientation::Landscape`，`dx=15`，`dy=0`）
- Raw framebuffer: `./assets/startup.framebuffer.bin`
- Panel-order framebuffer: `./assets/startup.panel.framebuffer.bin`
- PNG preview: `./assets/startup.preview.png`

![Host startup preview](./assets/startup.preview.png)

## 变更记录（Change log）

- 2026-04-10: 创建显示 bring-up 规格，冻结 async SPI、`panel_160x50`、host preview 与静态启动屏收口口径。
- 2026-04-10: 落地共享显示场景、host preview harness 与 Xtensa 异步 SPI 显示入口，并生成首版启动屏视觉证据。
- 2026-04-11: 根据实拍确认方向、偏移与颜色口径正确，最终固件先收口为静态启动屏常驻，同时保留冻结的 fan-cycle 行为。
- 2026-04-12: 启动后显示流程改为前面板最终界面轮播，移除测试场景常驻路径，并在硬件日志中确认实际轮播的是 Home / Preferences / Preset / Cooling / WiFi / Device 界面。

## 参考（References）

- [IvanLi-CN/gc9d01-rs](https://github.com/IvanLi-CN/gc9d01-rs)
- `firmware/src/board/s3_frontpanel.rs`
- `docs/hardware/s3-frontpanel-baseline.md`
