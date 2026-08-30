# Flux Purr 热控调优兼容性与演进

## Current Compatibility Boundary

`thermal_plant_auto`、RTD/VIN ADC 校准、heater curve 校准和普通运行时
`thermalProfileMode=auto|65w|100w` 保持各自既有职责。它们不是
`thermal_tuning_run_v1` 的替代入口，也不会在正式调优开始时被自动触发。

正式调优使用显式 `pps3a` / `pps5a`，其中 `pps3a` 覆盖 65W、`20V @ 3250mA`
等 3A-class PPS 合同。旧 firmware 未发布 capability 时明确不兼容；Web 不执行
host-reference fallback。

## Host Reference Retention

原有主机编排 `flux-purr thermal tune` 算法保留为 CLI 的
`--engine host-reference`。它是独立算法参考、受控 HIL fallback 与固件算法改进的
输入，不是过渡性死代码。删除、替换为同一 optimizer、或取消其可执行路径均需要主人
明确批准。

新产品 firmware runner 与 Web 不依赖外部电源遥测。reference-engine 可保留历史
bench diagnostics 的开发用途，但这些诊断不属于 `thermal-tuning-v2` 的生产证据，
也不进入控制台产品工作流。

## Report Compatibility

`thermal-tuning-v2` 是新的跨 surface 审查 bundle，包含完整 trace、candidate 和
decision ledger。`thermal-profile.accepted.json` 只保留 import compatibility；新
CLI runner 和 Web ZIP 不再把它作为正式导出或 candidate promotion 的依据。

## Ownership Migration

旧主机驱动的正式调优描述在新实现交付时迁移为 reference compatibility 描述。设备
成为 production live run 的唯一决策所有者；CLI 与 Web 分别持有本机记录器，`devd`
保持 transport/hardware service 边界。此迁移不得引入 Web 与 CLI 的进程、网络或
文件数据通信。
