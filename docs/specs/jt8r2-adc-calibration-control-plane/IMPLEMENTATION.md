# ADC 校准控制面实现状态（#jt8r2）

## Coverage

- 固件 `MemoryConfig` 已改为保存 channel-centered calibration state：共享样本、`slots.a` / `slots.b` 与 `active_slot`。
- EEPROM 迁移规则已固定：
  - 新共享样本集合取旧 `active` 样本
  - 槽位 `A` 取旧 `active fit`
  - 槽位 `B` 取旧 `draft fit`
  - 默认激活 `A`
  - 旧 `draft` 样本不迁移
- `adc_calibration_fit` 已改为新规则：
  - `0` 样本：`gain=1`、`offset=0`
  - `1` 样本：固定 `gain=1`，只算 `offset`
  - `>=2` 样本：共享样本线性拟合
- RTD raw ADC 先用于开路/短路故障判断，再对有效 ADC 读数应用当前激活槽位并换算温度。
- VIN ADC 由 `GPIO1` 采样，当前激活槽位校正后按分压比换算为 `status.voltageMv`。
- ADC1 pin 使用 `AdcCalBasic` 返回同次转换值，产品采样路径无条件移除高位 SAR 状态位并保留 12-bit code，再由独立 `AdcCalCurve` 逐样本换算 mV；`Status.adcDiagnostics` 贯通 USB JSONL、devd、CLI JSON 与 Web runtime contract。
- eFuse calibration version、init/reference code 或 reference mV 缺失时报告 `runtime_fallback`，不创建使用假定 1100 mV reference 的 curve，也不继续温度准确性验证。
- 冷启动矩阵未证明 measurement-status wait、禁用 Wi-Fi、提前初始化 ADC、请求负载隔离或单外设 quiet mode 能一致消除 residual raw-code movement。两个 ADC1 外部通道的 common-mode movement 在没有已知独立输入时不能区分 ADC transfer/reference 与外部模拟节点变化；不得据此加入环境温度、VIN、首次读数或时间曲线补偿。可复用诊断边界见 [`ESP32-S3 ADC absolute-accuracy diagnosis`](../../solutions/device-control/esp32-s3-adc-accuracy-diagnosis.md)。
- USB JSONL 已移除 `calibration_apply`，改为 `get_calibration` + `calibration_config`，并支持 `capture`、`delete`、`clear`、`import`、`set_active_slot`、`set_slot_fit`。
- `devd` 已暴露 `GET|PUT /api/v1/devices/:id/calibration`，不再保留 `POST /api/v1/devices/:id/calibration/apply`。
- `flux-purr calibration` CLI 已支持 get/capture/delete/clear/import/export/set-slot-fit/set-active-slot。
- Web 控制台已移除旧 `apply` 语义，状态卡与样本区改为“拟合建议值 + A/B 槽位 + 当前激活槽位”模型。
- RTD/VIN calibration samples 继续持久化 owner-entered physical references。RTD capture 使用 live `targetAdcMv` 作为 `expectedMv`，并原样保存 `referenceTempC` 与 `targetAdcMv`。
- `标定温度` 输入已经与硬件回读温度分离：硬件值只作为 placeholder/提示，不再自动写成 input value。
- 页面内离开已加 owner-facing guard：当任一标定模式仍处于 armed 状态时，切换顶层视图、切换设备或切换标定子 tab 会先在开关附近显示自定义提示泡泡。
- 三个校准模式现在都会在 `标定模式` 开启时自动接管 calibration-owned PPS；`PPS 电压` 滑块与数值输入以节流方式直接更新 runtime `ppsMv`。
- 校准页内的加热控制已统一为 Toggle 开关语义。
- 温度标定右上 `状态` 卡会把硬件回传的 `heaterOutputPercent` 渲染成能量强度图示，用来表达“实际是否正在加热”。
- 固件已为 `thermal_plant_auto` 建立独立的 `ThermalPlantRunSnapshot` 投影：运行序号、阶段、进度、温度、电压、占空比、受限轨迹点、预览曲线和已提交 active 结果彼此分离；轨迹通过 `afterSample` 分页，每页最多 16 点。
- USB、直接 LAN 和 `devd` native serial/LAN 路由共享 `thermal_plant_run` v1 字段与能力协商；mock 设备提供环境、加热、自然冷却和完成态样本，旧 `get_calibration_job` 端点继续保留。
- Web 加热曲线页已收口为单一自动校准命令和 C 版桌面结果卡：运行中展示阶段、进度、加热与自然冷却轨迹及预览，提交成功后展示四项模型指标、最终 R(T) 曲线和代表点记录；代表点表保持自然行高。

## Validation

- `cargo check -p flux-purr-firmware --quiet`
- `cargo test -p flux-purr-firmware --quiet`
- `cargo check -p flux-purr-devd --quiet`
- `cargo test -p flux-purr-devd --quiet`
- `./node_modules/.bin/tsc --noEmit --pretty false`
- `./node_modules/.bin/vitest --config vitest.unit.config.ts --run src/features/control-plane-demo/web-serial.test.ts src/features/control-plane-demo/transport-client.test.ts src/features/control-plane-demo/calibration-slider-value.test.ts`
- ESP32-S3 release build covers the production ADC diagnostics path.

## Remaining Work

- Storybook 已覆盖结果卡的完成、运行、失败和固件不兼容状态；旧 `active/draft/apply` 场景不再作为自动热模型入口。
- 桌面 `ui_demo` 视觉证据已归档，结果卡在 `1440x900` 下保留无页面或卡片滚动的 C 版证据布局。
- 真实硬件验收需主人另行提供精确且获授权的 MCU 端口。
