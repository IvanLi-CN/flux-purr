# ADC 校准控制面演进记录（#jt8r2）

## 2026-06-02

- 冻结 ADC 校准为 RTD ADC 与 VIN ADC 两个 channel，各保存最多 `8` 个 sample points。
- 明确校准点由物理参考值转换到 ADC-domain expected points，而不是显示层 offset。
- 明确 raw RTD 电气故障检查不受校准影响，VIN status 使用校准后的实测输入电压。

## 2026-06-25

- 页面内切换 calibration 顶层视图、设备或子 tab 时，若任一 calibration mode 仍然 armed，则必须先在开关附近显示自定义气泡提示，要求操作者先关闭开关，再允许继续跳转。
- RTD/VIN calibration sample 必须原样保留 owner-entered `referenceTempC` / `referenceVinMv`，页面、导入导出、设备回读与刷新后都优先显示该原值，而不是只靠 `expectedMv` 反推。
- RTD 温度标定样本还必须同时记录 capture 当下的硬件目标 `targetAdcMv`，并在样本表中和用户输入的标定温度并列显示。
- RTD 温度标定样本表收口为双栏配对布局，且每个样本只允许展示 `ADC 电压` 与 `温度` 两项数据。
- 温度标定页修正了“开启加热但硬件不出力”的语义错位：Web 不再自行阻断 `开启加热`，而是用硬件返回的 `heaterOutputPercent` 作为实际加热真相源并显示能量强度图示。
- 校准工作台进一步去掉了 owner-facing `申请 PPS` 步骤：进入任一校准模式后即自动接管 calibration-owned PPS，`PPS 电压` 滑块直接节流更新运行态电压。
- 三个校准模式中的加热控制统一收口为 Toggle 开关。

## 2026-06-30

- ADC 校准模型从旧 `active/draft/apply` 语义切换为“共享样本 + `A/B` 槽位 + `activeSlot`”。
- 样本集合与槽位职责彻底分离：样本只生成拟合建议值；硬件实际使用并持久化的是 `A/B` 槽位。
- 单点标定规则收口为固定 `gain=1`、只计算 `offset`；不再混入默认 identity point。
- 导入/导出升级为完整 calibration state；`calibration_apply` 与 HTTP apply endpoint 被移除。

## 2026-08-27

- 自动热模型运行状态从通用 `CalibrationJobState` 分离为 `ThermalPlantRunSnapshot`，以 `runId` 区分每次尝试，以 `afterSample` 游标分页返回受限瞬态轨迹。
- 快照明确区分 `ambient`、`heating`、`cooling` 阶段、运行中 provisional curve 与 EEPROM 提交后的 `activeResult`；自然冷却样本属于同一次运行的可见证据。
- USB、LAN、native `devd` 和 Web 使用同一 `thermal_plant_run` v1 契约，并通过 capability 协商避免旧固件重复报错。
