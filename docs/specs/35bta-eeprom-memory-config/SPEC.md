# Flux Purr EEPROM 记忆配置（#35bta）

## 状态

- Status: 已完成
- Created: 2026-04-27
- Last: 2026-04-27

## 背景 / 问题陈述

- 前面板已支持目标温度、`M1-M10` 记忆温度和主动降温策略，但这些值此前只存在于运行态，重启后回到默认值。
- S3 硬件基线已经冻结 `GPIO8/9` 共享 I2C 总线，并包含 `CH224Q` 与 `M24C64` EEPROM。
- 记忆配置需要在不影响 heater/fan 安全状态机的前提下跨重启恢复，并允许后续新增字段。

## 目标 / 非目标

### Goals

- 在 `M24C64` 外部 EEPROM 中保存版本化记忆配置。
- 保存并恢复 `target_temp_c`、`selected_preset_slot`、`presets_c[10]`、`active_cooling_enabled` 和 Wi-Fi 配置字段。
- 保存并恢复 ADC calibration 的共享样本、A/B 槽位与当前激活槽位，供 ADC 校准控制面跨重启保留。
- 使用双槽 record、TLV payload 和 CRC，保证坏数据自动回退默认值、未知字段可跳过。
- 运行时对用户接受的记忆字段变更做防抖写回，减少 EEPROM 写入频率。
- 保存与电流档无关的 heater raw observations 和瞬态 thermal plant model transaction。

### Non-goals

- 不保存 `heater_enabled`，重启后 heater 仍不得自动恢复加热。
- 不保存实时温度、fan runtime、fault latch、页面 route、菜单位置或蜂鸣器 reminder。
- 不实现运行时 PID 参数持久化。
- 不保存实时 ADC sample、实时温度、实时输入电压或 fault latch；ADC calibration 只保存共享样本和显式确认写入的槽位参数。
- 不对 Wi-Fi 密码做加密；但密码不得进入日志、前面板明文或状态输出。
- 不新增前面板菜单或改变现有视觉布局。

## 范围（Scope）

### In scope

- `firmware/src/memory.rs`
- `firmware/src/bin/flux_purr.rs`
- `firmware/src/lib.rs`
- `firmware/README.md`
- `docs/specs/35bta-eeprom-memory-config/**`

### Out of scope

- Web 控制台页面变更
- HTTP Wi-Fi 服务端实现
- EEPROM 工厂擦除/迁移命令

## 需求（Requirements）

### MUST

- EEPROM 设备默认为 `M24C64`，7-bit I2C 首选地址 `0x50`，固件在 `0x50..0x57` 范围内探测以兼容实板地址脚差异；容量 `8 KiB`，页写大小 `32 bytes`，16-bit word address。
- `MemoryRecord` 当前格式版本为 `v5`：header 的 byte `4` 保存 format version，byte `5` 保存 header length，bytes `6..8` 保存 payload length，bytes `8..12` 保存 `sequence`，bytes `12..16` 保存 CRC。v1-v5 均可解码；v1/v2 使用窄 TLV 长度，v3-v5 使用 `u16` TLV 长度。active 双槽位于 `0x1000` / `0x1800`，每槽 `2048 bytes`；previous 双槽为 `1024 bytes`（`0x0400` / `0x0800`），legacy 双槽为 `512 bytes`（`0x0000` / `0x0200`）。启动时选择 CRC 合法且 `sequence` 最大的 record。旧 EEPROM 槽只作为兼容读取源，选择出的旧配置先在 RAM 中完成字段迁移；后续成功提交配置时才以当前 v5 编码写入 active 槽，不在启动阶段强制重写 EEPROM。
- 外置 EEPROM 是主持久化后端；若 EEPROM 当前不可达或写入失败，固件必须使用 ESP flash 中标签为 `flux_cfg` 的专用 8KiB data partition 保存同一 `MemoryRecord`，不得直接占用或写入 NVS 管理范围。flash slot A/B 必须各自独占一个 4KiB erase sector，写入任一 slot 不得擦除另一份有效 record。启动时同时读取可用后端，选择 CRC 合法且 `sequence` 最大的 record；所有后端都无效时使用默认配置。升级前曾使用旧 factory-app 边界后 `0x110000` / `0x111000` raw 双槽的设备，若该区域仍含 CRC 合法 record，启动时必须将其迁移复制到当前 `flux_cfg`；新写入不得回到旧 raw 区域。
- `devd` 的 real-flash 路径在写入任何应用镜像前，必须读取设备当前 `0x8000` partition table 并解析实际 `flux_cfg`。若目标 [`firmware/partitions.csv`](../../../firmware/partitions.csv) 改变该分区地址，`devd` 必须将当前完整 `flux_cfg`（或未分区 legacy raw 双槽）预写到目标 `flux_cfg` 地址，再原样读回验证；只有验证成功才可写入新 partition table 和 app。目标地址被当前 layout 占用、目标容量较小、读取/解析失败或逐字验证不一致时，必须拒绝应用烧录。临时副本不得进入日志、trace 或用户配置目录。EEPROM 不受该 flash layout 迁移影响。
- record payload 必须使用 TLV，未知 TLV 必须跳过，缺失 TLV 必须使用默认值；v1/v2 的 TLV header 使用 `tag:u8 + len:u8`，v3-v5 使用 `tag:u8 + len:u16le`。
- 温度字段恢复后必须 clamp 到 `0..400°C`。
- `selected_preset_slot` 越界时必须回到默认槽位。
- 用户接受操作导致记忆字段变化时必须 debounce 后写回，不得每个按键事件立即写入持久化后端。
- EEPROM 读写失败不得阻断 heater/fan 保护逻辑；fallback flash 不可用时保存失败必须可见，但不得屏蔽安全保护。
- 日志不得输出 Wi-Fi 密码明文。

### SHOULD

- 写入下一槽而不是覆盖当前槽，降低掉电时同时破坏两份配置的概率。
- I2C 访问应复用现有 `GPIO8/9` 总线所有者，保持 CH224Q 与 EEPROM 串行访问。

## 功能与行为规格（Functional / Behavior Spec）

- 启动流程：
  - CH224Q 完成默认 PD 请求后，固件读取 EEPROM 与 flash fallback 中可用的记忆配置。
  - 创建 `FrontPanelUiState` 后，把记忆配置应用到目标温度、当前预设槽、预设数组和主动降温策略位。
  - `heater_enabled` 保持运行时默认/安全策略，不从 EEPROM 恢复。
- 写回流程：
  - 前面板已接受交互完成后，从 UI 状态生成下一份 `MemoryConfig`。
  - 若配置相对上一份有变化，设置约 `2s` 写回 deadline。
  - deadline 到期后写入下一 record sequence 对应的槽；EEPROM 不可用时写入 flash fallback；两者都失败则重新排队。
- Wi-Fi 字段：
  - `ssid`、`password`、`telemetryIntervalMs` 进入持久化模型；自动重连是固件固定策略，不属于用户配置。
  - 旧版本的 `wifi_auto_reconnect` TLV 继续读取以兼容已有记录，但加载与 sanitize 时始终归一化为 `true`。
  - 当前固件未实现 HTTP Wi-Fi 配置服务时，不额外虚构运行时联网行为。

## 接口契约（Interfaces & Contracts）

- `MemoryConfig` 是固件内部持久化模型。
- `M24c64` 是固件内部 EEPROM adapter，提供 bounded read 与 page-bounded write。
- Flash fallback 复用同一当前 v5 `MemoryRecord` 编码与 sequence 选择规则，存放在 ESP-IDF partition table 中标签为 `flux_cfg` 的专用 8KiB data partition；两个逻辑 slot 分别位于该分区的 `0x0000` 与 `0x1000`，只在 EEPROM 不可达或写入失败时使用。为兼容此前位于 `0x110000` / `0x111000` 的 raw fallback 双槽，当前 runtime 只读探测其 CRC 合法 record，并在发现时立即以 v5 编码复制到 `flux_cfg`；此兼容读取不写旧区域。当前 `flux_cfg` 中的旧格式 record 仍按版本解码，后续成功提交时物化为 v5。仓库根 `espflash.toml` 必须让 ELF 烧录同步写入 [`firmware/partitions.csv`](../../../firmware/partitions.csv)。支持 raw app artifact 时，devd 必须先写入由该 CSV 生成并受版本控制的 [`firmware/partitions.bin`](../../../firmware/partitions.bin) 到 `0x8000`，再写入 app 并显式 reset；两条安装路径都必须保证该分区属于正式 flash 合同。写入前 `devd` 以当前设备的 partition table 规划 `flux_cfg` 迁移；地址变化时先 read/verify 目标位置的完整 record，随后才允许安装目标 layout 和应用镜像。
- ADC calibration payload 固定编码 RTD/VIN 两个 channel，各 `8` 个共享 sample slot，并额外编码 `slots.a` / `slots.b` 的 `gain + offset` 以及 `activeSlot`。owner-facing physical reference 继续与 ADC-domain points 分离保存，保证刷新后仍可按原值显示。
- TLV 字段：
  - `0x01`: `target_temp_c` (`i16le`)
  - `0x02`: `selected_preset_slot` (`u8`)
  - `0x03`: `presets_c[10]` (`10 * i16le`，`i16::MIN` 表示 `---`)
  - `0x04`: `active_cooling_enabled` (`u8 bool`)
  - `0x10`: `wifi_ssid` (`utf8 bytes`)
  - `0x11`: `wifi_password` (`utf8 bytes`)
  - `0x12`: `wifi_auto_reconnect` (`u8 bool`, legacy compatibility; firmware always normalizes to `true`)
  - `0x13`: `telemetry_interval_ms` (`u32le`)
  - `0x20`: `adc_calibration_samples`
  - `0x21`: legacy draft ADC calibration samples
  - `0x22`: ADC calibration physical references
  - `0x23`: legacy draft ADC calibration physical references
  - `0x24`: ADC calibration targets
  - `0x25`: legacy draft ADC calibration targets
  - `0x26`: ADC calibration fit slots
  - `0x27`: ADC calibration active slots
  - `0x30`: legacy active thermal control profile
  - `0x31`: legacy active thermal control profile with current layout
  - `0x32`: `pps3a` saved thermal control profile
  - `0x33`: `pps5a` saved thermal control profile
  - `0x34`: `thermal_profile_mode` (`auto|65w|100w`)
  - `0x35`: `heater_curve_raw_observations`
  - `0x36`: legacy steady-state thermal-plant candidate record (decode-only)
  - `0x37`: legacy steady-state thermal-plant active record (decode-only)
  - `0x38`: LAN pairing token
  - `0x39`: static IPv4 configuration
  - `0x3a`: `thermal_plant_transient_active`
- 新记录持续写入 `0x32/0x33/0x34` 的两个 saved thermal profile 与 mode。`0x35` 保存 raw RTD ADC、实测 V/I/R；`0x36` 与 `0x37` 只保留为历史稳态双平台记录，绝不迁移或优先于新模型，也不得解锁加热。`0x3a` 保存瞬态 active 模型的 ambient raw RTD ADC、定长 `50ms` 轨迹、实测加热电压、duty、拟合系数和 transaction identity。派生温度、曲线与系数不得成为唯一持久化真相源。
- 新写入的 thermal profile payload 必须以紧凑 `TCP3` 布局标识开头；它无损保存完整 point-local 字段，并让两个十点 bank、最长 Wi-Fi 凭据、LAN token、static IPv4、完整 calibration 与最长瞬态轨迹共同装入一个 `2KiB` active record。`TCP2` 和无标识历史 payload 继续按各自旧布局优先解码。旧单档 thermal profile 自动迁移为 `pps3a`，且缺失 mode 时恢复为 `65w`。

## 验收标准（Acceptance Criteria）

- Given EEPROM 与 flash fallback 都为空或损坏，When 固件启动，Then UI 使用默认记忆配置且不 panic。
- Given 多个后端/槽都有合法 record，When 固件启动，Then 选择 `sequence` 最大的一槽。
- Given 最新槽 CRC 损坏且旧槽合法，When 固件启动，Then 回退到旧槽。
- Given 当前 `flux_cfg` 没有有效 record 且旧 raw fallback 双槽仍有 CRC 合法 record，When 固件启动，Then 恢复该 record 并以当前 v5 编码复制到当前 `flux_cfg`。
- Given EEPROM previous/legacy 槽存在 CRC 合法的 v1-v4 record，When 固件启动，Then 按版本完成 RAM 内字段迁移并恢复配置；旧 EEPROM 槽不在启动阶段被重写，下一次成功配置提交必须以 v5 编码写入 active 槽。
- Given 当前设备的 `flux_cfg` 位于旧 factory app 末尾且目标 app partition 会覆盖该地址，When `devd` 执行 real flash，Then 必须先在未分配的目标 `flux_cfg` 地址写入并逐字验证完整原始 record，验证成功后才写入 app；任一 preflight 步骤失败时 app 不得写入。
- Given record payload 包含未知 TLV，When 解码，Then 忽略未知字段并保留已知字段。
- Given 目标温度或 preset 超出范围，When 解码完成，Then 温度被 clamp 到 `0..400°C`。
- Given 用户修改目标温度、preset 或主动降温策略，When 约 `2s` debounce 到期，Then 写入下一持久化槽。
- Given heater 曾在重启前开启，When 固件重启，Then heater 不因持久化配置自动开启。
- Given ADC calibration state 已写入持久化后端，When 固件重启，Then 共享样本、A/B 槽位与当前激活槽位都恢复。
- Given ADC calibration sample 在保存时带有 `referenceTempC` 或 `referenceVinMv`，When 固件重启或 control-plane 重新读取 calibration package，Then ADC-domain points 与原始 physical reference 都恢复，页面不需要靠 `expectedMv` 反推 owner-facing 标定值。
- Given EEPROM record 来自旧格式且没有 `0x22/0x23/0x24/0x25` reference/target TLV，When 固件解码，Then calibration sample 仍恢复为同样的 `observed_mv/expected_mv`，只是缺失的 reference/target 字段使用默认值。
- Given v1 或 legacy record 只含一个 saved thermal profile，When 解码，Then profile 写入 `pps3a` bank，`pps5a` 保持空 profile，mode 为 `65w`。
- Given v2 record 同时含两个 thermal bank，When 重启恢复，Then 两个 bank、mode、Wi-Fi 凭据和 calibration state 都完整恢复。
- Given 无标识历史 thermal profile 的 payload 长度与新 point-local 布局长度相同，When 固件升级后解码，Then 必须优先恢复历史 settings/point 布局，不得按新布局错位读取。
- Given RTD calibration active slot or fit changes, When memory is read again, Then raw heater and
  thermal observations remain byte-for-byte stable and all derived values are rebuilt from the new
  projection.
- Given a transient thermal trace does not contain an ordered powered rise to `220°C` followed by
  zero-duty cooling to `80°C`, or its physical projection cannot be formed, When calibration ends,
  Then it leaves the existing active transaction unchanged and heating remains locked.

## 非功能性验收 / 质量门槛（Quality Gates）

- `cargo test --manifest-path firmware/Cargo.toml`
- `cargo fmt --manifest-path firmware/Cargo.toml --check`
- Xtensa build: `source /Users/ivan/export-esp.sh && cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin flux-purr --release`

## 文档更新（Docs to Update）

- `firmware/README.md`
- `docs/specs/README.md`
- `docs/specs/35bta-eeprom-memory-config/**`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增 EEPROM 记忆格式、TLV 编解码、CRC 与双槽选择测试
- [x] M2: 新增 M24C64 adapter 并接入启动恢复
- [x] M3: 接入运行时 dirty tracking 与 debounce 写回
- [x] M4: 更新文档并完成验证 / review 收敛

## 方案概述（Approach, high-level）

- 把格式逻辑放在 `firmware/src/memory.rs`，用 host 单测覆盖坏数据、未知字段和边界校验。
- ESP32 runtime 只在主循环里串行访问 EEPROM 与 CH224Q 共享 I2C，避免并发总线仲裁复杂度。
- 以 TLV 为后续扩展点，新增字段只追加 tag，不改变旧字段含义。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 假设：M24C64 地址脚按硬件基线配置为 7-bit 地址 `0x50`；固件仍会探测 `0x50..0x57`。当实机 EEPROM 不响应时，flash fallback 必须维持保存/重启恢复能力，避免调优和校准流程被单一外设阻断。
- 风险：当前实现未加密 Wi-Fi 密码；若后续威胁模型要求物理攻击防护，需要另开安全存储规格。
- 风险：若后续新增更多高频配置项，需要重新评估 EEPROM 写入寿命与合并写策略。

## 参考（References）

- `../233y7-c3-ch224q-ch442e-frontpanel/SPEC.md`
- `../fk3u7-frontpanel-input-interaction/SPEC.md`
- `../q2aw6-heater-pid-frontpanel-runtime/SPEC.md`
