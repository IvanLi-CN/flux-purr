# Flux Purr 真实控制平面运行时实现状态（#m8r4q）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实。

## Current Status

- Implementation: 进行中
- Lifecycle: active
- Catalog note: Web + firmware + native devd real transport contract

## Coverage / rollout summary

- 共享领域契约由 firmware、devd 与 Web 各自的 typed adapter 实现。
- firmware v1 先交付 host-testable status adapter、USB JSONL parser/encoder、WiFi redaction、runtime config 与 feature flags。
- `tools/flux-purr-devd` 提供 localhost daemon、授权端口 serial discovery、lease、bounded events、USB identity/network/status/WiFi/runtime bridge、artifact verify、dry-run 与 flash command boundary。
- `devd` artifact catalog 从当前 repo build outputs 计算 ESP32-S3 与 host binary 的 size、sha256、flash address；development CORS 支持 Vite JSON preflight。
- Web app 保持 `#hhwq8` 轻量 bench console，新增 transport client、capability gate、live devd discovery bridge 与 artifact catalog/verify dry-check；验证入口为 Vite Web App，不使用 ControlPlaneDemo Storybook 页面。
- 主工作区既有真机 smoke 已覆盖 ESP32-S3 release build、`devd` USB 设备枚举、lease/status/WiFi redaction、artifact verify、dry-run guard、`mcu-agentd` flash 与 reset monitor；当前 Web-to-devd-to-firmware runtime bridge 仍需在授权端口在线时复验。
- `devd` real flash path 绑定 lease 对应 native serial port；空 artifact 不再被 dry-run 视为通过。

## Remaining Gaps

- PR 号在 PR 创建后回填。
- 当前授权端口 `/dev/cu.usbmodem21221401` 不存在时，devd 不暴露 native serial target，Web 只能使用 mock target；真实硬件读写、WiFi provisioning 与 runtime control 不能标记为完成。
- Web Update 已能加载 `devd` catalog 并通过 `POST /api/v1/artifacts/verify` 执行 dry-check；真实 flash 仍需授权端口在线并显式允许真实写入后复验。
- 完整 artifact catalog 管理页不属于本 spec 范围。

## Hardware Smoke

- Device selector: `/dev/cu.usbmodem21221401`
- Build: `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3,web_serial,net_http --bin flux-purr --release`
- Artifact: `firmware/target/xtensa-esp32s3-none-elf/release/flux-purr`
- Artifact SHA-256: `76a9d5ad5f034339a77a00e76c625b271a063e796121bc82ab50a75ee1c6db22`
- `mcu-agentd --non-interactive flash esp32s3_frontpanel`: passed with `status=0`.
- Flash session: `.mcu-agentd/sessions/esp32s3_frontpanel/20260523_175059.session.ndjson`
- `mcu-agentd --non-interactive monitor esp32s3_frontpanel --reset`: captured startup and runtime logs.
- Monitor session: `.mcu-agentd/monitor/esp32s3_frontpanel/20260523_175127_512.mon.ndjson`
- Observed runtime: frontpanel app mode, CH224Q detected at `0x22`, PPS available, heater backend `pps-mos`, RTD/VIN samples flowing, dashboard UI loop stable, no fault reported during smoke.

## References

- `./SPEC.md`
- `../../solutions/device-control/web-native-wifi-bridge-console.md`
- `../hhwq8-web-control-plane-demo/SPEC.md`
