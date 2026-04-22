# Flux Purr S3FH4R2 + CH224Q 直连前面板基线（移除 CH442E / TCA6408A）（#233y7）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-04-22

## 背景 / 问题陈述

- `flux-purr` 现有分配在 `ESP32-C3` 上已经被 BOOT 键、LCD 背光 PWM、`FAN_EN` 直连和输入采样需求挤到很紧，继续堆约束会让板级设计变得别扭。
- 当前主人已明确切换到 `ESP32-S3FH4R2`，并要求移除 `TCA6408A`，改为前面板与显示控制信号全部直连 MCU。
- 若不统一更新硬件基线、firmware board profile 与对外契约，后续实现会继续沿用过时的 `C3 + expander` 假设。

## 目标 / 非目标

### Goals

- 冻结 `ESP32-S3FH4R2 + CH224Q` 的直连前面板 GPIO 分配。
- 从 baseline 与 firmware 中移除 `TCA6408A` 相关假设与适配层。
- 保持 CH224Q 控制、输入电压采样、风扇控制、前面板按键、LCD 控制的契约一致。
- 为 RGB 状态灯冻结三路独立 PWM GPIO 分配，并与硬件文档/firmware board profile 对齐。
- 按 `normal-flow` 收口至本地 PR-ready（不 push、不建 PR）。

### Non-goals

- 不进行真实硬件烧录、下载模式联调或示波器验证。
- 不在本轮引入新的前面板协议、触摸方案或键盘矩阵。
- 不改写 Web/API 的字段形态，只同步底层硬件口径。

## 范围（Scope）

### In scope

- `docs/specs/233y7-c3-ch224q-ch442e-frontpanel/SPEC.md` 与索引更新。
- 新增 `docs/hardware/s3-frontpanel-baseline.md`，删除旧的 C3 基线。
- 新增 heater 与 power-tree 相关设计文档并与基线互相链接。
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
- firmware-active GPIO 分配固定为以下 24 路且无重复：`0,1,2,8,9,10,11,12,13,14,15,16,17,18,19,20,21,35,36,37,38,39,47,48`。
- `GPIO0` 必须直连前面板中键并承担 `BOOT` 键角色，采用 active-low 连接。
- `GPIO10/11/12/13` 应尽量对齐 `mains-aegis` 的 LCD cluster，其中 `GPIO10=LCD_DC`、`GPIO11=LCD_MOSI`、`GPIO12=LCD_SCLK`、`GPIO13=LCD_BLK`。
- `GPIO13` 必须直接输出 PWM 到 `LCD_BLK`。
- `GPIO47`（芯片 pin `37`）必须保留为 heater PWM 输出。
- `GPIO48`（芯片 pin `36`）必须保留为 buzzer PWM / beep 输出。
- `GPIO35` 必须直接拥有风扇 `EN` 控制路径，允许在 MCU 侧原始控制网与实际 `FAN_EN` 之间插入保护/串联电阻；`GPIO36` 必须直连 `FAN_PWM`。
- `GPIO37/38/39` 必须分别冻结为 `RGB_B_PWM`、`RGB_G_PWM`、`RGB_R_PWM` 三路独立状态灯 PWM 输出。
- `GPIO1` / `ADC1_CH0` 用于 `VIN` 采样，延续 `56 kOhm / 5.1 kOhm` 分压方案。
- `GPIO2` / `ADC1_CH1` 用于 `PT1000` 采样。
- `PT1000` 直连 ADC 的基线外围固定为：`R_REF=2.49 kOhm (0.1%)`、`R_SERIES=2.2 kOhm`、`C_ADC=100 nF`，并在 MCU ADC 侧增加低漏电 ESD 钳位。
- `GPIO8/9` 用作共享 I2C，总线上至少包含 `CH224Q` 与一颗 `M24C64` EEPROM。
- `GPIO19/20` 用于原生 USB `D-/D+`。
- `GPIO34` 可作为硬件接入的 `FAN_TACH` 输入存在，但它不计入当前 firmware-active 的 24 路 GPIO 集。
- 保留 `DeviceStatus` 中的 `frontpanel_key`、`pd_request_mv`、`pd_contract_mv`、`fan_enabled`、`fan_pwm_permille` 字段。
- 固定 `3.3 V` 电源应使用输入欠压锁定，目标行为为：约 `4.5 V` 以下关断、约 `5.0 V` 恢复。

### SHOULD

- 避开 `ESP32-S3` 的 strapping pins `GPIO3`、`GPIO45`、`GPIO46`。
- 避开 `GPIO26 ~ GPIO32` 的 flash / PSRAM 占用区。
- 若继续沿用默认 USB Serial/JTAG 路径，则允许把 `GPIO39`（封装信号 `MTCK`）复用为普通 PWM 输出；若未来改为外部 GPIO JTAG，则必须重新审视 RGB_R 分配。
- `FAN_EN` 默认通过弱下拉保持关闭。
- `BOOT(GPIO0)` 采用 released-high / pressed-low 的标准电路。

### COULD

- 后续可在 firmware 中新增直接按键采样辅助类型或去抖逻辑。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 固件启动时加载 `ESP32-S3FH4R2` board profile，并校验固定 GPIO 表无重复。
- CH224Q 通过 `GPIO8/9` 共享 I2C 总线完成地址识别与寄存器编码控制字生成。
- `M24C64` EEPROM 与 CH224Q 共用 `GPIO8/9` I2C，总线地址规划与固件仲裁必须兼容这一共享结构。
- 共享 `GPIO8/9` I2C 总线保留 `4.7 kOhm` 到 `3V3` 的上拉。
- 前面板四向键与中键分别由 MCU 直接读取，不依赖 expander。
- LCD `DC/MOSI/SCLK/BLK` 与 `mains-aegis` 对齐为 `GPIO10/11/12/13`，`RES/CS` 继续由 MCU 直连，其中 `BLK` 支持 PWM。
- Buzzer 输出由 `GPIO48` 提供；固件可将其作为普通 beep GPIO 或 PWM/LEDC 音调输出使用。
- RGB 状态灯由 `GPIO39/38/37` 提供 `R/G/B` 三路 PWM；固件可按状态机需要输出静态颜色或亮度调制。
- `PT1000` 通过 `GPIO2` 进入 MCU ADC；固件按校准后的 ADC 电压换算温度，开路/短路应视为故障态而不是有效温度。
- `GPIO35` 的风扇使能控制在实现上可以表现为 `FAN_EN_RAW -> series resistor -> FAN_EN`，但 firmware 仍将其视为单一使能输出所有权。
- 设备状态快照继续输出 `frontpanel_key` 与 PD/风扇字段。

### Edge cases / errors

- I2C 地址不在 `0x22/0x23` 时必须返回错误。
- GPIO 表一旦重复或总数不是 `21`，单测必须失败。
- 直连键位模型保留 `center | right | down | left | up | null`，但不再假设任何 expander 输入位图。
- 若热探头最终确认是 `PT100` 而不是 `PT1000`，则当前直连 ADC 方案失效，必须切换到专用 RTD 前端。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Device REST API | HTTP | external | Modify | ../../interfaces/http-api.md | firmware + web | web console | `board` 示例切换为 `esp32-s3` |
| Device status model | Rust type | internal | Modify | /firmware/src/lib.rs | firmware | firmware + web mock | 移除 expander 类型依赖，保留键值语义 |

### 契约文档（按 Kind 拆分）

- `../../interfaces/http-api.md`

## 验收标准（Acceptance Criteria）

- Given `ESP32-S3FH4R2` board profile 已落地，When 运行 `gpio_map_is_valid`，Then 测试通过且 GPIO 总数为 24 且不重复。
- Given RGB 状态灯 GPIO 分配已冻结，When 检查 board profile 常量，Then `RGB_B/G/R` 分别固定为 `GPIO37/38/39`。
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
- `docs/hardware/heater-power-switch-design.md`
- `docs/hardware/tps62933-dual-rail-power-design.md`
- `README.md`
- `firmware/README.md`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: S3 直连硬件基线与规格索引更新
- [x] M2: firmware board profile 切到 S3 并移除 `TCA6408A`
- [x] M3: README / HTTP 契约同步

## 方案概述（Approach, high-level）

- 使用 `ESP32-S3FH4R2` 释放 GPIO 预算，直接满足 BOOT 键、LCD 背光 PWM、风扇控制与前面板按键直连。
- Heater 功率开关保持为独立的低边 `NMOS` 方案，并通过单独硬件设计文档冻结外围与验证要求。
- Buzzer 单独占用芯片 pin `36` / `GPIO48`，与 heater 的 pin `37` / `GPIO47` 形成相邻的高编号控制输出对，便于布线。
- LCD 与风扇控制的 pin map 尽量向 `mains-aegis` 靠拢，降低后续跨项目复用与查线成本。
- RGB 状态灯使用相邻的 `GPIO37/38/39` 组成三路 PWM 输出，便于布线并减少与既有锁定功能的交叉。
- 温度探头沿用 mini-hotplate 参考资料里的 `PT1000` 假设，这样可以继续使用 MCU 直连 ADC，而不必额外加 RTD 专用芯片。
- 避开 `ESP32-S3` 的 strapping pins 与 flash / PSRAM 占用区，保持板级余量和 bring-up 清晰度。
- 保持对外 API 语义稳定，仅更新板级来源说明。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：切到 `ESP32-S3FH4R2` 后，firmware 实际目标工具链应回到 `xtensa-esp32s3-none-elf`。
- 风险：移除 expander 后，前面板按键去抖和唤醒行为都变成 MCU 侧职责。
- 风险：实机上仍需验证 `GPIO0` 中键与 USB 下载流程是否满足期望的人机交互。
- 风险：`GPIO35/36/37/38/39/47/48` 虽在 `ESP32-S3FH4R2` 可用集合内，但仍需结合最终 PCB 布局确认走线与 EMI 余量。
- 风险：`PT1000` 直连 ADC 的方案偏向控制/保护用途，若主人后续要求高精度绝对温度，则仍应升级到专用 RTD 前端。
- 假设：当前显示面板接受 `LCD_BLK` 的 MCU 直连 PWM 驱动。
- 假设：`GPIO8/9` 的共享 I2C 总线由 CH224Q 与 M24C64 EEPROM 共同占用，后续若再挂载外设需要重新审视地址与时序预算。
- 假设：温度探头最终确认为 `PT1000` 而不是 `PT100`。
- 假设：`GPIO34` 上的 `FAN_TACH` 在当前 revision 里只冻结为硬件输入，firmware 可后续再接入。
- 假设：项目继续使用默认 USB Serial/JTAG 调试路径，不把 JTAG eFuse 切到 `GPIO39~42`。

## 变更记录（Change log）

- 2026-03-03: 创建原始前面板基线规格。
- 2026-03-27: C3 口径下移除 CH442E 并补充 VIN 采样方案。
- 2026-03-28: 切换为 `ESP32-S3FH4R2`，移除 `TCA6408A`，改为直连前面板与 LCD 控制。
- 2026-03-28: 参考 `mains-aegis` 重新收敛 LCD 与风扇控制引脚，统一到 `GPIO10/11/12/13` 与 `GPIO35/36`。
- 2026-03-28: 明确 `GPIO2` 作为 `PT1000` 直连 ADC 输入，并冻结 RTD 偏置/滤波外围值。
- 2026-03-30: 补充双路 `TPS62933DRLR` 电源与 heater 低边 `NMOS` 的独立设计文档，并与主基线交叉链接。
- 2026-03-30: 因布线需求，将 `HEATER_PWM` 迁移到芯片 pin `37` / `GPIO47`。
- 2026-03-31: 新增 buzzer 输出分配到芯片 pin `36` / `GPIO48`。
- 2026-04-22: 按主人确认的原理图片段新增 RGB 状态灯分配：`GPIO39/38/37 -> RGB_R/G/B_PWM`。

## 参考（References）

- [ESP32-S3 GPIO Guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/gpio.html)
- [ESP32-S3 Boot Mode Selection](https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/boot-mode-selection.html)
- [ESP32-S3 USB Device Guide](https://docs.espressif.com/projects/esp-idf/en/release-v5.5/esp32s3/api-reference/peripherals/usb_device.html)
- [../../hardware/tps62933-dual-rail-power-design.md](../../hardware/tps62933-dual-rail-power-design.md)
- [../../hardware/heater-power-switch-design.md](../../hardware/heater-power-switch-design.md)
