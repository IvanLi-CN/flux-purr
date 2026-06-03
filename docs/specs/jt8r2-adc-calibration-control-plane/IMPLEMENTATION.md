# ADC 校准控制面实现状态（#jt8r2）

## Coverage

- 固件 `MemoryConfig` 保存 `active_adc_calibration` 与 `draft_adc_calibration`，TLV record 同时编码两份 calibration package。
- `adc_calibration_fit` 按 `0/1/>=2` custom point 规则计算线性 gain/offset；`correct_adc_mv` 在 ADC 域修正 observed millivolts。
- RTD raw ADC 先用于开路/短路故障判断，再对有效 ADC 读数应用 active calibration 并换算温度。
- VIN ADC 由 `GPIO1` 采样，active calibration 后按分压比换算为 `status.voltageMv`。
- USB JSONL 支持 `get_calibration`、`calibration_config` 与 `calibration_apply`；启动/恢复窗口对 mutating calibration frame 返回可重试 busy/error。
- `devd` exposes `GET|PUT /api/v1/devices/:id/calibration` and `POST /api/v1/devices/:id/calibration/apply` with mock and native serial paths.
- `flux-purr calibration` 支持 get/capture/delete/clear/import/export/apply。
- Web 控制台包含 Calibration tab，并在 Storybook 中覆盖 Calibration 场景。

## Validation

- `bun run check:firmware:fmt`
- `bun run check:firmware:clippy`
- `bun run check:firmware:build`
- `bun run check:devd`
- `bun run check:web`
- `bun run check:web:build`
- `bun run check:storybook`

## Remaining Work

- 真机校准需要主人授权具体 USB 端口后执行 HIL smoke。
- 前面板本机校准菜单不属于当前控制面。

