# 固件调优 60°C 冷却起点交付合同

## 范围与基线

本合同仅适用于 `thermal tune --engine firmware` 的 `pps5a` 60°C 目标，覆盖首次试验及每次重试。基线为固件主控 5A 调优交付合同及其冻结的外部主控策略。无 `--engine` 的 `thermal tune` 仍执行未修改的外部主控方案。

获准差异是：设备实测温度降至 `<=40°C` 即可开始固件 60°C 试验，而外部主控仍要求 `<=35°C`。固件的其它默认目标继续要求 `currentTempC <= targetTempC - 40°C`。不得从此例外推导出其它策略或生产控制差异。

## 不变约束

- Cooldown 期间逻辑加热请求与最终 MOS PWM 均保持零。所选 PPS 请求和已确认 5A 合同均达到 APDO 上限后才可开始试验；仅满足温度条件不得加热。
- 60°C 的首次试验和所有重试都使用 `<=40°C`；门槛首次满足时立即开始，不增加稳定等待。
- 单目标 `20min` 预算仍从 cooldown 开始，覆盖所有试验、重试、PPS 准备和 hold confirm；不得增加独立轮次上限或重置预算。
- 候选生成、seed/插值、评分、hard gate、accepted-target blocking、生产 PID/profile 选择、preview 和 EEPROM 保存保持不变。过冲 `<=3°C`、hold p2p `<=3°C`、full-speed-to-stable 和 `60s` hold 门槛均不得放宽。
- 不修改 PD/PPS 协议或 PowerState 语义；`pps3a` 仍不得加热或产生调优候选。

## 证据与验收

- 每个已开始的固件试验在 `candidate_trial` 事件中记录 cooldown→warmup 边界的设备实测温度和当时门槛。标准五文件报告将两项事实写入该试验数据，并随选中试验展示。历史记录缺少事实时显示 unavailable；渲染器不得用当前策略倒推旧运行的门槛。
- 单元测试证明 60°C 首次及重试试验在 `40.00°C` 开始、`40.01°C` 继续等待；80°C 及以上保持 `target - 40°C`。外部主控回归测试证明其 60°C 仍要求 `<=35°C`。
- 授权 `pps5a` 实机 HIL 须证明 60°C 试验在 `<=40°C` 启动、cooldown 输出为零且 PPS 合同已确认，之后按不变的 gate 如实通过或生成完整失败报告。此例外本身不等于九目标资格通过。
