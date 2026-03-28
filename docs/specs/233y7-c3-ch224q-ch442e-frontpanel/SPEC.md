# Flux Purr S3FH4R2 + CH224Q 直连前面板基线（移除 CH442E / TCA6408A）（#233y7）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-28

## 背景 / 问题陈述

- `flux-purr` 现有分配在 `ESP32-C3` 上已经被 BOOT 键、LCD 背光 PWM、`FAN_EN` 直连和输入采样需求挤到很紧，继续堆约束会让板级设计变得别扭。
- 当前主人已明确切换到 `ESP32-S3FH4R2`，并要求移除 `TCA6408A`，改为前面板与显示控制信号全部直连 MCU。
- 若不统一更新硬件基线、firmware board profile 与对外契约，后续实现会继续沿用过时的 `C3 + expander` 假设。

## 目标 / 非目标

### Goals

- 冻结 `ESP32-S3FH4R2 + CH224Q` 的直连前面板 GPIO 分配。
- 从 baseline 与 firmware 中移除 `TCA6408A` 相关假设与适配层。
- 保持 CH224Q 控制、输入电压采样、风扇控制、前面板按键、LCD 控制的契约一致。
- 按 `normal-flow` 收口至本地 PR-ready（不 push、不建 PR）。

### Non-goals

- 不进行真实硬件烧录、下载模式联调或示波器验证。
- 不在本轮引入新的前面板协议、触摸方案或键盘矩阵。
- 不改写 Web/API 的字段形态，只同步底层硬件口径。

## 范围（Scope）

### In scope

- `docs/specs/233y7-c3-ch224q-ch442e-frontpanel/SPEC.md` 与索引更新。
- 新增 `docs/hardware/s3-frontpanel-baseline.md`，删除旧的 C3 基线。
- `firmware/` 内 board profile 更新为 `ESP32-S3FH4R2`，移除 `TCA6408A` 适配层。
- `docs/interfaces/http-api.md`、`README.md`、`firmware/README.md` 口径同步。

### Out of scope

- 真实 LCD 驱动、按键扫描、CH224Q I2C 事务层实现。
- USB DFU / TinyUSB 功能落地。
- Web UI 新字段或新控件设计。

## 需求（Requirements）

### MUST

- MCU 切换为 `ESP32-S3FH4R2`。
- 前面板不再使用 `TCA6408A`，所有按键与 LCD 控制脚改为 MCU 直连。
- GPIO 分配固定为以下 20 路且无重复：`0,1,2,5,8,9,10,11,12,13,14,15,16,17,18,19,20,21,35,36`。
- `GPIO0` 必须直连前面板中键并承担 `BOOT` 键角色，采用 active-low 连接。
- `GPIO10/11/12/13` 应尽量对齐 `mains-aegis` 的 LCD cluster，其中 `GPIO10=LCD_DC`、`GPIO11=LCD_MOSI`、`GPIO12=LCD_SCLK`、`GPIO13=LCD_BLK`。
- `GPIO13` 必须直接输出 PWM 到 `LCD_BLK`。
- `GPIO35` 必须直连 `FAN_EN`，`GPIO36` 必须直连 `FAN_PWM`。
- `GPIO1` / `ADC1_CH0` 用于 `VIN` 采样，延续 `56 kOhm / 5.1 kOhm` 分压方案。
- `GPIO2` / `ADC1_CH1` 用于温度采样。
- `GPIO8/9` 仅用于 `CH224Q` I2C。
- `GPIO19/20` 用于原生 USB `D-/D+`。
- 保留 `DeviceStatus` 中的 `frontpanel_key`、`pd_request_mv`、`pd_contract_mv`、`fan_enabled`、`fan_pwm_permille` 字段。

### SHOULD

- 避开 `ESP32-S3` 的 strapping pins `GPIO3`、`GPIO45`、`GPIO46`。
- 避开 `GPIO26 ~ GPIO32` 的 flash / PSRAM 占用区。
- `FAN_EN` 默认通过弱下拉保持关闭。
- `BOOT(GPIO0)` 采用 released-high / pressed-low 的标准电路。

### COULD

- 后续可在 firmware 中新增直接按键采样辅助类型或去抖逻辑。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 固件启动时加载 `ESP32-S3FH4R2` board profile，并校验固定 GPIO 表无重复。
- CH224Q 通过 `GPIO8/9` I2C 地址识别与寄存器编码生成控制字。
- 前面板四向键与中键分别由 MCU 直接读取，不依赖 expander。
- LCD `DC/MOSI/SCLK/BLK` 与 `mains-aegis` 对齐为 `GPIO10/11/12/13`，`RES/CS` 继续由 MCU 直连，其中 `BLK` 支持 PWM。
- 设备状态快照继续输出 `frontpanel_key` 与 PD/风扇字段。

### Edge cases / errors

- I2C 地址不在 `0x22/0x23` 时必须返回错误。
- GPIO 表一旦重复或总数不是 `20`，单测必须失败。
- 直连键位模型保留 `center | right | down | left | up | null`，但不再假设任何 expander 输入位图。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Device REST API | HTTP | external | Modify | ../../interfaces/http-api.md | firmware + web | web console | `board` 示例切换为 `esp32-s3` |
| Device status model | Rust type | internal | Modify | /firmware/src/lib.rs | firmware | firmware + web mock | 移除 expander 类型依赖，保留键值语义 |

### 契约文档（按 Kind 拆分）

- `../../interfaces/http-api.md`

## 验收标准（Acceptance Criteria）

- Given `ESP32-S3FH4R2` board profile 已落地，When 运行 `gpio_map_is_valid`，Then 测试通过且 GPIO 总数为 20 且不重复。
- Given CH224Q 适配层，When 对 `0x22/0x23` 进行解析并编码 `5/9/12/15/20/28V`，Then 地址解析与寄存器编码结果正确。
- Given VIN sense 方案，When 按 `56 kOhm / 5.1 kOhm` 计算 `28V` 输入，Then ADC 引脚电压不高于 `2.337V`。
- Given firmware 不再依赖 `TCA6408A`，When 构建 `firmware` crate，Then 不再存在 `tca6408a` 模块引用。
- Given 契约与示例已同步，When 检查 `docs/interfaces/http-api.md`、`README.md`、`firmware/README.md`，Then 均指向 `ESP32-S3` 口径。

## 实现前置条件（Definition of Ready / Preconditions）

- 主人已确认 MCU 切换到 `ESP32-S3FH4R2`。
- 主人已确认去掉 `TCA6408A`。
- 主人已确认 `LCD_BLK` 必须由 MCU 直接输出 PWM。
- 本规格中的直连引脚口径已冻结。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `cargo test --manifest-path firmware/Cargo.toml`
- Integration tests: N/A（本轮为主机侧逻辑适配）

### Quality checks

- `bash scripts/check-firmware-fmt.sh`
- `bash scripts/check-firmware-clippy.sh`
- `bash scripts/check-firmware-build.sh`

## 文档更新（Docs to Update）

- `docs/specs/README.md`
- `docs/interfaces/http-api.md`
- `docs/hardware/s3-frontpanel-baseline.md`
- `README.md`
- `firmware/README.md`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: S3 直连硬件基线与规格索引更新
- [x] M2: firmware board profile 切到 S3 并移除 `TCA6408A`
- [x] M3: README / HTTP 契约同步

## 方案概述（Approach, high-level）

- 使用 `ESP32-S3FH4R2` 释放 GPIO 预算，直接满足 BOOT 键、LCD 背光 PWM、风扇控制与前面板按键直连。
- LCD 与风扇控制的 pin map 尽量向 `mains-aegis` 靠拢，降低后续跨项目复用与查线成本。
- 避开 `ESP32-S3` 的 strapping pins 与 flash / PSRAM 占用区，保持板级余量和 bring-up 清晰度。
- 保持对外 API 语义稳定，仅更新板级来源说明。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：切到 `ESP32-S3FH4R2` 后，firmware 实际目标工具链应回到 `xtensa-esp32s3-none-elf`。
- 风险：移除 expander 后，前面板按键去抖和唤醒行为都变成 MCU 侧职责。
- 风险：实机上仍需验证 `GPIO0` 中键与 USB 下载流程是否满足期望的人机交互。
- 风险：`GPIO35/36` 虽在 `ESP32-S3FH4R2` 可用集合内，但仍需结合最终 PCB 布局确认走线与 EMI 余量。
- 假设：当前显示面板接受 `LCD_BLK` 的 MCU 直连 PWM 驱动。
- 假设：`GPIO8/9` 专用于 CH224Q，不再复用其他 I2C 外设。
- 假设：保留 `GPIO34` 为空脚位有助于未来补入 `FAN_TACH`。

## 变更记录（Change log）

- 2026-03-03: 创建原始前面板基线规格。
- 2026-03-27: C3 口径下移除 CH442E 并补充 VIN 采样方案。
- 2026-03-28: 切换为 `ESP32-S3FH4R2`，移除 `TCA6408A`，改为直连前面板与 LCD 控制。
- 2026-03-28: 参考 `mains-aegis` 重新收敛 LCD 与风扇控制引脚，统一到 `GPIO10/11/12/13` 与 `GPIO35/36`。

## 参考（References）

- [ESP32-S3 GPIO Guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/gpio.html)
- [ESP32-S3 Boot Mode Selection](https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/boot-mode-selection.html)
- [ESP32-S3 USB Device Guide](https://docs.espressif.com/projects/esp-idf/en/release-v5.5/esp32s3/api-reference/peripherals/usb_device.html)
