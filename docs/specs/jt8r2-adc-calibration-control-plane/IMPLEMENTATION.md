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

## Validation

- `cargo check -p flux-purr-firmware --quiet`
- `cargo test -p flux-purr-firmware --quiet`
- `cargo check -p flux-purr-devd --quiet`
- `cargo test -p flux-purr-devd --quiet`
- `./node_modules/.bin/tsc --noEmit --pretty false`
- `./node_modules/.bin/vitest --config vitest.unit.config.ts --run src/features/control-plane-demo/web-serial.test.ts src/features/control-plane-demo/transport-client.test.ts src/features/control-plane-demo/calibration-slider-value.test.ts`

## Remaining Work

- 更新 Storybook 场景到完整的新 `A/B` 槽位语义。
- 更新 Playwright 场景，去掉对旧 `active/draft/apply` 行为的假设。
- 完成浏览器视觉证据与真机验收。
