# Flux Purr S3 风扇循环调速 bring-up（#8tesd）

## 状态

- Status: 已完成
- Created: 2026-04-09
- Last: 2026-04-09

## 背景 / 问题陈述

- 当前主线固件只有 `no_std` host-side mock，缺少可烧录的 `ESP32-S3` bring-up 入口。
- 已验证的同仓库 S3 frontpanel 基线表明：风扇控制走 `GPIO35=FAN_EN`、`GPIO36=FAN_PWM`、`GPIO34=FAN_TACH` 预留，且 `GPIO36` 通过 `TPS62933` 反馈注入调节风扇电压而不是直接做线级 PWM。
- 若不先落地最小风扇 runtime，就无法对已焊接样机做基础通电行为验证，也无法建立后续 tach / 热控 / UI 联调的固件基线。

## 目标 / 非目标

### Goals

- 为 `ESP32-S3` 新增一个最小可构建的风扇 bring-up binary。
- 在固件域模型中冻结四相风扇循环：`10s high -> 10s low -> 10s mid -> 10s stop`。
- 把 `FAN_EN/FAN_PWM/FAN_TACH` 的板级常量与基础 GPIO map 固化到 `firmware/src/board/`。
- 保持 host 侧 `fmt` / `clippy` / `build` / `test` 继续可跑，同时补齐 `cargo +esp` 的 `xtensa-esp32s3-none-elf` 构建口径。

### Non-goals

- 不做实机烧录、示波器波形、转速/听感/热行为实测。
- 不接入 `GPIO34` tach 输入，不做闭环调速或失速检测。
- 不扩展 heater、PT1000、按键、LCD、CH224Q 的真实驱动实现。
- 不修改 Web UI 或新增外部 API 契约。

## 范围（Scope）

### In scope

- `docs/specs/8tesd-s3-fan-cycle-bringup/SPEC.md` 与 `docs/specs/README.md`。
- `.cargo/config.toml` 中的 Xtensa build-std / linker 配置。
- `firmware/src/lib.rs` 中的风扇域模型、状态字段与单测。
- `firmware/src/board/mod.rs` 与 `firmware/src/board/s3_frontpanel.rs`。
- `firmware/src/bin/esp32s3_fan_cycle.rs` 的最小 S3 bring-up 入口。
- `firmware/Cargo.toml`、`firmware/README.md` 的构建与运行说明。

### Out of scope

- 其它板级文档迁移或整套 S3 frontpanel 全功能接线落地。
- 量产参数、保护阈值和调速曲线优化。
- PR 合并后的 cleanup 以外的后续功能开发。

## 需求（Requirements）

### MUST

- `FAN_EN` 冻结为 `GPIO35`，`FAN_PWM` 冻结为 `GPIO36`，`FAN_TACH` 冻结为 `GPIO34`。
- 风扇循环顺序固定为 `high -> low -> mid -> stop`，每相 `10s`。
- 默认档位冻结为：`high=30‰`、`mid=300‰`、`low=500‰`，`stop` 必须通过 `EN low` 实现。
- `GPIO36` 的 LEDC 频率固定目标为 `25kHz`。
- 风扇电压映射必须保留反相控制律：占空比越高，目标输出电压越低。
- host 构建必须不依赖 xtensa 依赖链；xtensa runtime 必须在 `esp32s3 + xtensa` 条件下单独可构建。

### SHOULD

- LEDC 优先尝试 `10-bit` 分辨率，不成立时自动回退 `8-bit`。
- `stop` 相位保留安全默认 PWM 值，但不得等价于“低速档”。
- 风扇域模型保持纯 Rust、可单测，不把硬件访问逻辑塞进库层状态机。

### COULD

- 后续在相同域模型上扩展 tach 采样、测速和失速恢复。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 固件 binary 上电后先配置 `GPIO36` LEDC，再控制 `GPIO35` 使能风扇电源。
- 启动相位为 `high`，保持 10 秒后依次切换到 `low`、`mid`、`stop`，然后循环。
- `high/mid/low` 相位下 `FAN_EN=true`，`stop` 相位下 `FAN_EN=false`。
- host 侧 `snapshot()` 继续返回 mock 状态，但风扇字段必须反映同样的四相循环语义。

### Edge cases / errors

- 若 `25kHz + 10-bit` 的 LEDC timer 配置失败，必须自动尝试 `8-bit` 回退，而不是直接 panic。
- `stop` 状态不得通过“保留 EN 高、占空比调低”来冒充停机。
- 若外部以大步长时间推进控制器，状态机仍必须跨越多个 10 秒边界并落到正确相位。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DeviceStatus fan fields | Rust type | internal | Modify | None | firmware | firmware tests / future adapters | 新增 `fan_enabled`、`fan_pwm_permille` |
| esp32s3 fan bring-up binary | CLI/binary | internal | New | None | firmware | bring-up operator | 仅用于样机基础通电调速 |

### 契约文档（按 Kind 拆分）

None

## 验收标准（Acceptance Criteria）

- Given `FanCycleController` 从 `0s` 开始，When 在 `10/20/30/40s` 边界读取命令，Then 相位依次为 `low/mid/stop/high`。
- Given 默认档位 `30‰/300‰/500‰`，When 计算近似风扇输出电压，Then 结果分别约为 `5.0V/4.4V/4.0V` 且误差保持在小容差内。
- Given `stop` 相位，When 读取命令，Then `enabled=false` 且不把停机等价成低速档。
- Given S3 board 常量，When 运行 GPIO map 测试，Then `FAN_EN/FAN_PWM/FAN_TACH` 分别固定为 `35/36/34`。
- Given host 质量门，When 运行 `cargo test`、`fmt`、`clippy`、`build`，Then 全部通过。
- Given Xtensa 目标构建，When 运行 `cargo +esp build --target xtensa-esp32s3-none-elf --features esp32s3 --bin esp32s3-fan-cycle --release`，Then 构建成功。

## 实现前置条件（Definition of Ready / Preconditions）

- S3 frontpanel 风扇控制口径已冻结为 `GPIO35/36/34`。
- 10 秒四相循环顺序与默认档位已冻结。
- host 与 xtensa 双构建口径已明确分离。
- 本轮不做 tach / heater / RTD / LCD 真实驱动已明确。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `cargo test --manifest-path firmware/Cargo.toml`
- Integration tests: N/A（本轮仅 host 逻辑 + Xtensa 构建）
- E2E tests (if applicable): N/A

### Quality checks

- `bash scripts/check-firmware-fmt.sh`
- `bash scripts/check-firmware-clippy.sh`
- `bash scripts/check-firmware-build.sh`
- `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin esp32s3-fan-cycle --release`

## 文档更新（Docs to Update）

- `docs/specs/README.md`: 新增本规格索引行，并在收口时更新状态/PR 备注。
- `.cargo/config.toml`: 让 `cargo +esp --manifest-path firmware/Cargo.toml ...` 在仓库根目录直接命中 Xtensa 构建配置。
- `firmware/README.md`: 补充 `esp32s3-fan-cycle` build/run 说明与引脚口径。
- `firmware/Cargo.toml`: 同步 xtensa runtime 依赖与 target feature 说明。

## 计划资产（Plan assets）

- None

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 冻结 S3 风扇板级常量与 spec/index
- [x] M2: 落地风扇域模型、状态字段与 host 单测
- [x] M3: 落地 `esp32s3-fan-cycle` binary 并通过 host/Xtensa 构建验证

## 方案概述（Approach, high-level）

- 先把风扇时序抽成纯 Rust 状态机，再由 xtensa binary 负责把命令映射到 `GPIO35 + GPIO36`。
- Host 构建路径保留无硬件依赖的 stub，避免现有仓库门禁被 xtensa-only 依赖拖挂。
- LEDC 只承担稳定 PWM 输出；10 秒循环用最小 bring-up runtime 控制，不在本轮引入更复杂的 async executor。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：样机真实低速档可能在 `~4.0V` 附近出现起转不稳，后续 bench 可能需要上调。
- 风险：`25kHz + 10-bit` 在最终依赖版本上可能不成立，因此需要 `8-bit` 回退路径。
- 风险：当前 `stop` 相位只验证逻辑语义，未验证实际 fan rail 放电时间常数。
- 需要决策的问题：None（本轮实现口径已冻结）。
- 假设（需主人确认）：同仓库远端 S3 frontpanel 基线仍是当前最强硬件真相源。

## 变更记录（Change log）

- 2026-04-09: 创建风扇循环调速 bring-up 规格并冻结四相循环、板级引脚与验证口径。
- 2026-04-09: 完成 S3 风扇 bring-up 实现，补齐仓库根 Xtensa 构建配置并通过 host/Xtensa 验证。

## 参考（References）

- `remotes/origin/th/c3-ch224q-ch442e-frontpanel:docs/specs/233y7-c3-ch224q-ch442e-frontpanel/SPEC.md`
- `remotes/origin/th/c3-ch224q-ch442e-frontpanel:docs/hardware/s3-frontpanel-baseline.md`
- `remotes/origin/th/c3-ch224q-ch442e-frontpanel:docs/hardware/tps62933-dual-rail-power-design.md`
- [esp-hal 1.0.0 LEDC docs](https://docs.espressif.com/projects/rust/esp-hal/1.0.0/esp32/esp_hal/ledc/index)
