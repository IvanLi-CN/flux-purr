# ADC 校准控制面演进记录（#jt8r2）

## 2026-06-02

- 冻结 ADC 校准为 RTD ADC 与 VIN ADC 两个 channel，各保存最多 `8` 个 sample points。
- 明确校准点由物理参考值转换到 ADC-domain expected points，而不是显示层 offset。
- 明确 draft/active 双包持久化、显式 apply 和 heater-active apply 阻断。
- 明确 raw RTD 电气故障检查不受校准影响，VIN status 使用校准后的实测输入电压。
