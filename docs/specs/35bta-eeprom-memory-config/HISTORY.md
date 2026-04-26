# EEPROM 记忆配置演进记录（#35bta）

## 2026-04-27

- 冻结 EEPROM 记忆配置为 `M24C64` 双槽 record + TLV payload + CRC。
- 明确保存用户偏好，不保存 heater arm、故障、fan runtime、页面 route 等运行态安全状态。
- 采用 debounce 写回，避免前面板每次按键立即写 EEPROM。
