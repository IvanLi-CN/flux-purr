# EEPROM 记忆配置演进记录（#35bta）

## 2026-04-27

- 冻结 EEPROM 记忆配置为 `M24C64` 双槽 record + TLV payload + CRC。
- 明确保存用户偏好，不保存 heater arm、故障、fan runtime、页面 route 等运行态安全状态。
- 采用 debounce 写回，避免前面板每次按键立即写 EEPROM。

## 2026-06-02

- 将 ADC calibration active/draft packages 纳入 `MemoryConfig` TLV payload。
- 保持实时 ADC sample、实时温度、电压与 fault latch 不进入 EEPROM。

## 2026-06-25

- ADC calibration EEPROM payload 拆分为 ADC-domain sample TLV 与 physical-reference TLV，允许 RTD/VIN 样本在重启后同时恢复 `observed/expected` 和原始 `referenceTempC` / `referenceVinMv`。
- 旧格式 record 若缺少 reference TLV，解码后继续保留既有 ADC-domain 样本，并把 reference 字段视为空值。
