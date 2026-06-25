# ADC 校准控制面演进记录（#jt8r2）

## 2026-06-02

- 冻结 ADC 校准为 RTD ADC 与 VIN ADC 两个 channel，各保存最多 `8` 个 sample points。
- 明确校准点由物理参考值转换到 ADC-domain expected points，而不是显示层 offset。
- 明确 draft/active 双包持久化、显式 apply 和 heater-active apply 阻断。
- 明确 raw RTD 电气故障检查不受校准影响，VIN status 使用校准后的实测输入电压。

## 2026-06-25

- 页面内切换 calibration 顶层视图、设备或子 tab 时，若任一 calibration mode 仍然 armed，则必须先在开关附近显示自定义气泡提示，要求操作者先关闭开关，再允许继续跳转。
- RTD/VIN calibration sample 必须原样保留 owner-entered `referenceTempC` / `referenceVinMv`，页面、导入导出、设备回读与刷新后都优先显示该原值，而不是只靠 `expectedMv` 反推。
- RTD 温度标定样本还必须同时记录 capture 当下的硬件目标 `targetAdcMv`，并在样本表中和用户输入的标定温度并列显示。
