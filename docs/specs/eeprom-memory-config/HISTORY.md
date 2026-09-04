# EEPROM 记忆配置演进记录

## Legacy identity

- Former legacy ID: `35bta`.

## Lifecycle

- `active`: EEPROM persistence remains part of the current product contract.

- 双槽扩展为 `1024 bytes` 并迁移到 `0x0400` / `0x0800`，以保证完整校准、最长 Wi-Fi 凭据和六点热控 profile 能同时编码。启动恢复在新槽均无有效 record 时只读旧 `512 bytes` 双槽，后续提交自动写入新槽。

## 2026-04-27

- 冻结 EEPROM 记忆配置为 `M24C64` 双槽 record + TLV payload + CRC。
- 明确保存用户偏好，不保存 heater arm、故障、fan runtime、页面 route 等运行态安全状态。
- 采用 debounce 写回，避免前面板每次按键立即写 EEPROM。

## 2026-06-02

- 将 ADC calibration 纳入 `MemoryConfig` TLV payload，并作为后续共享样本 + A/B 槽位模型的持久化基础。
- 保持实时 ADC sample、实时温度、电压与 fault latch 不进入 EEPROM。

## 2026-06-25

- ADC calibration EEPROM payload 拆分为 ADC-domain sample TLV 与 physical-reference TLV，允许 RTD/VIN 样本在重启后同时恢复 `observed/expected` 和原始 `referenceTempC` / `referenceVinMv`。
- 旧格式 record 若缺少 reference TLV，解码后继续保留既有 ADC-domain 样本，并把 reference 字段视为空值。
- Active EEPROM records use redundant `2 KiB` slots with current v5 `u16` TLV lengths so both thermal profile banks can retain ten complete point-local parameter sets; `1 KiB` previous and `512 B` legacy slots remain read-only migration sources.

## 2026-08-04

- `MemoryRecord` header version is v5. Firmware decodes v1-v5, migrates legacy fields in RAM, and writes v5 on the next successful configuration commit. EEPROM old slots are not eagerly rewritten during boot; legacy raw flash fallback is copied to `flux_cfg` immediately when discovered.

## 2026-07-26

- Flash fallback moved from unmanaged raw writes at the end of NVS to the dedicated `flux_cfg` 8KiB data partition installed by the repository `espflash` partition-table contract.
- Host full-batch tuning now rejects target sets above the firmware profile capacity before any HIL work starts.
