# Flux Purr EEPROM 记忆配置

## 背景 / 问题陈述

- 前面板已支持目标温度、`M1-M10` 记忆温度和主动降温策略，但这些值此前只存在于运行态，重启后回到默认值。
- S3 硬件基线已经冻结 `GPIO8/9` 共享 I2C 总线，并包含 `CH224Q` 与 `M24C64` EEPROM。
- 记忆配置需要在不影响 heater/fan 安全状态机的前提下跨重启恢复，并允许后续新增字段。

## 目标 / 非目标

### Goals

- 在 `M24C64` 外部 EEPROM 中保存版本化记忆配置。
- 保存并恢复 `target_temp_c`、`selected_preset_slot`、`presets_c[10]`、`active_cooling_enabled` 和 Wi-Fi 配置字段。
- 保存并恢复 ADC calibration 的共享样本、A/B 槽位与当前激活槽位，供 ADC 校准控制面跨重启保留。
- 使用 FPR2 分类 record、TLV payload 和 CRC；只有安全校准、温控策略与布局标记使用 A/B，偏好和网络域使用单槽。
- 运行时对用户接受的记忆字段变更做防抖写回，减少 EEPROM 写入频率。
- 保存安全校准所需的 raw heater observations 与 transaction identity；已废弃 thermal-plant snapshot 不迁移、不再写入。

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
- `docs/specs/eeprom-memory-config/**`

### Out of scope

- Web 控制台页面变更
- HTTP Wi-Fi 服务端实现
- 面向普通用户的 EEPROM 恢复流程、迁移包或 Web/LAN 维护入口

## 需求（Requirements）

### MUST

- EEPROM 设备为 `M24C64`，7-bit I2C 地址固定为硬件基线 `0x50`；启动和高级维护不得扫描其它 I2C 地址。容量 `8 KiB`，页写大小 `32 bytes`，16-bit word address。启动读取使用固定大小的域缓冲和不超过 `16 bytes` 的有界分块访问，不得把完整 `2 KiB` v5 快照放入启动栈或堆。
- FPR2 header 固定为 `20 bytes`：`FPR2 magic`、format version、domain、flags、header length、`sequence`、payload length、reserved 和 CRC32。分区固定为：`SafetyCalibration` A/B=`0x0000/0x0200`（512 B）；`ThermalPolicy` A/B=`0x0400/0x0700`（768 B）；`UserPreferences` single=`0x0a00`（128 B）；`NetworkAndPairing` single=`0x0a80`（256 B）；`LayoutMarker` A/B=`0x0c00/0x0c80`（128 B）；`0x0d00..0x0fff` 保留。旧 v1-v5 FPM1 只在迁移时流式读取，位于 `0x1000..0x1fff`，迁移后两个 magic 均失效。每个域选择 CRC 合法且 `sequence` 最大的槽；单槽损坏只回退该域默认值。
- 外置 EEPROM 是唯一的持久化后端。`MemoryRecord`、等价配置与其任何镜像不得写入 ESP flash、NVS、raw sector 或 `flux_cfg`。启动只从 EEPROM 槽位选择 CRC 合法且 `sequence` 最大的 record；旧内部 Flash record 必须忽略且不得迁移。EEPROM 全空时可按批准的硬件配置初始化并写后验证；EEPROM 不可达或安全域记录无法恢复时进入 `EEPROM_REQUIRED`，普通偏好/网络域失败只标记该域未保存。
- 固件更新、恢复与 devd 不得读取、保存、迁移、恢复或验证 MCU 内部配置分区。分区表、镜像布局和 bundle 不得声明 `flux_cfg` 或等价配置区域。
- record payload 必须使用 TLV，未知 TLV 必须跳过，缺失 TLV 必须使用默认值；v1/v2 的 TLV header 使用 `tag:u8 + len:u8`，v3-v5 使用 `tag:u8 + len:u16le`。
- 温度字段恢复后必须 clamp 到 `0..400°C`。
- `selected_preset_slot` 越界时必须回到默认槽位。
- 用户接受操作导致记忆字段变化时必须 debounce 后写回，不得每个按键事件立即写入持久化后端。
- EEPROM 读写失败不得阻断 heater/fan 保护逻辑。安全校准或温控策略失败时锁定 heater/PPS/calibration，候选值在读回验证前不得生效；偏好或网络失败只标记该域未保存，不锁定 heater，重启时该域回退默认值。EEPROM 错误页是唯一 retry 入口，中键长按触发一次重试，其他按键只清除提示并继续导航。
- 每次持久化提交失败必须保留 `code`、`phase`、`attempt`、`sequence`、目标 `slot`（`A`、`B` 或 `single`）和脱敏 `message`；`PersistenceFault` 字段与 devd JSONL 形状保持兼容。
- 持久化故障必须通过串口打印 `PERSISTENCE_COMMIT_ATTEMPT_FAILED`（每次尝试）和 `PERSISTENCE_COMMIT_FAILED`（终态）暴露相同的分类元数据；不得输出 EEPROM 原始字节、Wi-Fi 密码或其它敏感配置。
- M24C64 与 FUSB302B 共用 `GPIO8/9` 时，record 写入和成功后的 EEPROM 验证必须以不超过 `16 bytes` 的 bounded chunk 执行；每个 EEPROM write-cycle delay 或验证 chunk 后必须先释放 EEPROM adapter 并服务 PD，再开始下一段。EEPROM 成功即完成本次持久化，不得同步 mirror 到任何 MCU 存储。
- 日志不得输出 Wi-Fi 密码明文。
- EEPROM 含有非 `0xFF` 数据但所有受支持槽都无法解码、CRC/结构无效或格式版本高于当前固件时，固件必须锁定 heater、PPS 与 calibration，并在前面板固定显示 `EEPROM DATA`、`INCOMPATIBLE`、`HEATER LOCKED`。全 `0xFF` EEPROM 视为空白，不显示该场景。
- USB JSONL 与仓库 devd CLI 必须提供高级原始维护操作：按 offset/length 有界读取、按 offset 原样写入和全片擦除。导出和导入必须逐字节覆盖完整 `8 KiB`，不得解析、迁移、过滤或绑定设备身份；原始字节不得写入 transport event 日志。原始写入或擦除开始前必须清除 debug/calibration PPS、锁定 heater/calibration、请求 fixed PD，并清除所有普通 record 写回 deadline；传输或验证失败后保持该锁，避免部分镜像重新供热或被普通持久化覆盖。擦除必须写入并回读验证全片 `0xFF`，且不得自动创建默认 record。

### SHOULD

- 写入下一槽而不是覆盖当前槽，降低掉电时同时破坏两份配置的概率。
- I2C 访问应复用现有 `GPIO8/9` 总线所有者，保持 CH224Q 与 EEPROM 串行访问。

## 功能与行为规格（Functional / Behavior Spec）

- 启动流程：
- CH224Q 完成默认 PD 请求后，固件只读取 EEPROM 中可用的记忆配置。
  - 创建 `FrontPanelUiState` 后，把记忆配置应用到目标温度、当前预设槽、预设数组和主动降温策略位。
  - `heater_enabled` 保持运行时默认/安全策略，不从 EEPROM 恢复。
- 写回流程：
  - 前面板已接受交互完成后，从 UI 状态生成下一份 `MemoryConfig`。
  - 若配置相对上一份有变化，设置约 `2s` 写回 deadline。
- deadline 到期后按变更域写入下一 record sequence 对应的槽；每页 EEPROM 写和验证 chunk 后先服务共享总线上的 PD，再进入下一段。EEPROM 不可用、写入失败或验证失败不得重新路由到 MCU 存储。
- 提交失败时状态接口公开 `persistenceFault` 与 `persistenceFaultAttentionPending`，安装状态公开 `lastPersistenceFault`；`recordState` 使用 `valid|blank|corrupt|incompatible|unavailable`，不可使用 `eeprom_required` 作为记录状态。
- Wi-Fi 字段：
  - `ssid`、`password`、`telemetryIntervalMs` 进入持久化模型；自动重连是固件固定策略，不属于用户配置。
  - 旧版本的 `wifi_auto_reconnect` TLV 继续读取以兼容已有记录，但加载与 sanitize 时始终归一化为 `true`。
  - 当前固件未实现 HTTP Wi-Fi 配置服务时，不额外虚构运行时联网行为。

## 接口契约（Interfaces & Contracts）

- `MemoryConfig` 是固件内部持久化模型。
- `M24c64` 是固件内部 EEPROM adapter，提供 bounded read 与 page-bounded write。
- EEPROM 原始维护仅通过 USB/devd lease 暴露，不进入设备 LAN API 或 Web 控制台；它是避免 EEPROM 数据丢失的高级兜底工具，不属于普通用户工作流。物理 heater 输出非零时固件必须拒绝维护操作。
- MCU Flash、NVS、raw sector、`flux_cfg` 与它们的 layout migration 不属于 `MemoryConfig` 合同。仓库根 `espflash.toml` 与支持的镜像布局不得安装配置 fallback 分区；历史内部 Flash record 不得读取、解码或迁移。EEPROM 在 MCU Flash 写入、擦除和恢复操作外，独立保持其物理内容。
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
  - `0x3a`: legacy `thermal_plant_transient_active` (decode-only)
  - `0x3b`: `heater_curve_transaction_id`
- FPR2 只把 `0x32/0x33/0x34` 的两个 saved thermal profile 与 mode 写入 `ThermalPolicy`，把 `0x35`、`0x3b` 与 commissioning/ADC 字段写入 `SafetyCalibration`，把偏好和网络字段分别写入对应单槽域。`0x36`、`0x37` 与 `0x3a` 只保留为历史稳态/瞬态 thermal-plant 数据的 decode-only 标签，绝不迁移、不再写入，也不得解锁加热；旧记录中的派生模型只用于兼容读取和诊断。
- 新写入的 thermal profile payload 必须以紧凑 `TCP3` 布局标识开头，两个 bank 独立存入 `ThermalPolicy` A/B 槽。`TCP2` 和无标识历史 payload 继续按各自旧布局优先解码。旧单档 thermal profile 自动迁移为 `pps3a`，且缺失 mode 时恢复为 `65w`。

## 验收标准（Acceptance Criteria）

- Given EEPROM 为空且可写，When 固件启动，Then 固件从批准的硬件配置初始化 EEPROM、验证写入，并在 UI 使用该配置。
- Given EEPROM 缺失，或安全校准/温控策略记录不可读、不可写或验证失败，When 固件启动或提交配置，Then 固件进入 `EEPROM_REQUIRED`，不使用内部 Flash/NVS/raw sector 且 heater/PPS/calibration 保持锁定；Given 偏好/网络域失败，Then 只标记该域未保存并允许 heater/fan 保护继续运行。
- Given EEPROM 槽都有合法 record，When 固件启动，Then 选择 `sequence` 最大的一槽。
- Given 最新槽 CRC 损坏且旧槽合法，When 固件启动，Then 回退到旧槽。
- Given `flux_cfg` 或旧 raw fallback 双槽含 CRC 合法 record，When 固件启动，Then 固件忽略它们，绝不读取、恢复或复制。
- Given EEPROM previous/legacy 槽存在 CRC 合法的 v1-v5 record，When 固件启动，Then 按版本流式完成 RAM 内字段迁移、逐域写入并读回验证，依次提交 `PREPARED`、使旧 magic 无效、提交两份 `ACTIVE`；任一中断阶段重启都不得把半成品当作已激活配置。
- Given firmware update、Developer flash 或 MCU Flash recovery 发生，When MCU 写入或擦除完成，Then 操作不得读取、写入、迁移或验证内部配置分区，且外置 EEPROM 不受该 MCU 操作影响。
- Given record payload 包含未知 TLV，When 解码，Then 忽略未知字段并保留已知字段。
- Given 目标温度或 preset 超出范围，When 解码完成，Then 温度被 clamp 到 `0..400°C`。
- Given 用户修改目标温度、preset 或主动降温策略，When 约 `2s` debounce 到期，Then 写入下一持久化槽。
- Given FUSB302B 与 EEPROM 共享 I2C 且一个 record 需要多页写入，When debounce 或 WiFi 配置触发持久化，Then 每个写或验证 chunk 后都必须轮询 PD，且 EEPROM 成功不得触发同步 flash mirror。
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
- `docs/specs/eeprom-memory-config/**`

## 方案概述（Approach, high-level）

- 把格式逻辑放在 `firmware/src/memory.rs`，用 host 单测覆盖坏数据、未知字段和边界校验。
- ESP32 runtime 只在主循环里串行访问 EEPROM 与 CH224Q 共享 I2C，避免并发总线仲裁复杂度。
- 以 TLV 为后续扩展点，新增字段只追加 tag，不改变旧字段含义。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 假设：M24C64 地址脚按硬件基线固定为 7-bit 地址 `0x50`；固件只访问该硬件地址。当实机 EEPROM 不响应时，固件进入 `EEPROM_REQUIRED`，直到经过独立硬件与安全批准的非持久化默认配置可用。
- 风险：当前实现未加密 Wi-Fi 密码；若后续威胁模型要求物理攻击防护，需要另开安全存储规格。
- 风险：若后续新增更多高频配置项，需要重新评估 EEPROM 写入寿命与合并写策略。

## Related ADRs

- [`../../adr/0008-eeprom-only-configuration-persistence.md`](../../adr/0008-eeprom-only-configuration-persistence.md)
- [`../../adr/0009-eeprom-record-class-persistence.md`](../../adr/0009-eeprom-record-class-persistence.md)

## 参考（References）

- `../s3-ch224q-frontpanel-baseline/SPEC.md`
- `../frontpanel-input-interaction/SPEC.md`
- `../heater-pid-frontpanel-runtime/SPEC.md`
