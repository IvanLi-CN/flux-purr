# ADC 校准控制面实现状态（#jt8r2）

## Coverage

- 固件 `MemoryConfig` 保存 `active_adc_calibration` 与 `draft_adc_calibration`，TLV record 同时编码两份 calibration package。
- `adc_calibration_fit` 按 `0/1/>=2` custom point 规则计算线性 gain/offset；`correct_adc_mv` 在 ADC 域修正 observed millivolts。
- RTD raw ADC 先用于开路/短路故障判断，再对有效 ADC 读数应用 active calibration 并换算温度。
- VIN ADC 由 `GPIO1` 采样，active calibration 后按分压比换算为 `status.voltageMv`。
- Runtime status now also exposes latest raw RTD/VIN ADC millivolts so host-side calibration data packet collection can record uncalibrated samples alongside calibrated status.
- USB JSONL 支持 `get_calibration`、`calibration_config` 与 `calibration_apply`；启动/恢复窗口对 mutating calibration frame 返回可重试 busy/error。
- `devd` exposes `GET|PUT /api/v1/devices/:id/calibration` and `POST /api/v1/devices/:id/calibration/apply` with mock and native serial paths.
- `flux-purr calibration` 支持 get/capture/delete/clear/import/export/apply。
- Web 控制台包含 Calibration tab，并在 Storybook 中覆盖 Calibration 场景。
- RTD/VIN calibration samples now persist owner-entered physical references alongside ADC-domain points, and RTD samples also persist the live hardware `targetAdcMv`, so refreshed/reloaded sample tables can render the original `referenceTempC` / `referenceVinMv` plus the captured RTD hold target instead of reverse-derived placeholders.
- 页面内离开已加 owner-facing guard：当任一标定模式仍处于 armed 状态时，切换顶层视图、切换设备或切换标定子 tab 会先在开关附近显示自定义提示泡泡，要求先关闭开关，再允许继续跳转；本轮不拦截浏览器刷新或关页。

## Validation

- `bun run check:firmware:fmt`
- `bun run check:firmware:clippy`
- `bun run check:firmware:build`
- `bun run check:devd`
- `bun run check:web`
- `bun run check:web:build`
- `bun run check:storybook`
- `bun run --cwd web typecheck`
- `bun run --cwd web test:unit -- src/features/control-plane-demo/calibration-leave-guard.test.ts src/features/control-plane-demo/control-plane-demo.test.ts`
- `bun run --cwd web test:storybook -- src/stories/ControlPlaneDemo.stories.tsx`

## Remaining Work

- 真机校准需要主人授权具体 USB 端口后执行 HIL smoke。
- 前面板本机校准菜单不属于当前控制面。
