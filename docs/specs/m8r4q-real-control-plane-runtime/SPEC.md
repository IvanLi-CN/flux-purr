# Flux Purr 真实控制平面运行时（#m8r4q）

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，关键演进原因见 `./HISTORY.md`。

## 背景 / 问题陈述

- PR #27 已把 Web、native USB daemon、USB CDC、WiFi provisioning、firmware flashing 与 monitoring 的长期架构沉淀到 `docs/solutions/device-control/web-native-wifi-bridge-console.md`。
- `#hhwq8` 只冻结 mock-first Web demo，不代表真实 transport、daemon、USB CDC、WiFi HTTP 或真实 flashing 已交付。
- 本规范冻结 Flux Purr 真实控制面 v1：同一领域契约通过 mock、USB serial 和 native `devd` 暴露，Web 只根据能力启用操作；direct firmware HTTP 属于后续 `net_http` server 能力，不得在固件实现前声明。

## 目标 / 非目标

### Goals

- 定义 Web、firmware 与 `devd` 共享的 identity、network、status、USB JSONL、devd HTTP、firmware artifact 与 error envelope。
- firmware 提供 feature-gated `web_serial` contract adapter，复用现有热控 runtime 和 EEPROM WiFi 字段；direct `net_http` 只有在固件 HTTP server 落地后才可声明。
- `devd` 作为 localhost HTTP daemon，提供 USB/serial discovery、lease、monitor、WiFi bridge、artifact verify、dry-run 与 real flash command boundary。
- Web demo 保持轻量 bench console 形态，但通过 client/transport 层接入 mock 与 devd contract，并对危险操作做 capability gate。
- 无真机时必须能用 host tests、mock serial 和 devd dry-run 验证主要契约。

### Non-goals

- 不把 `#hhwq8` 扩张成 fleet 管理后台。
- 不在无硬件环境声明 USB/WiFi/flash 已完成真机验证。
- 不绕过 lease、confirmation、dry-run、artifact verification 或 secret redaction。
- 不把 WiFi PSK、host path 细节或 raw secret trace 写入 UI history、logs 或 errors。

## 范围（Scope）

### In scope

- `firmware/src/control_plane.rs` 及 feature flags。
- `tools/flux-purr-devd/**` native daemon。
- `web/src/features/control-plane-demo/contracts.ts` 与 transport client。
- `docs/interfaces/http-api.md` 当前 HTTP/devd/USB contract。

### Out of scope

- host power actions。
- 用户认证、多租户 fleet 管理和远端云服务。

## 需求（Requirements）

### MUST

- 所有 transport 暴露同一领域模型：`Identity`、`NetworkSummary`、`Status`、`FirmwareArtifact`、`ApiError`。
- USB serial frame 使用 newline-delimited JSON；需要响应的 request 必须带 `request_id`。
- `hello` 必须返回 protocol version、framing、identity 和 capabilities。
- WiFi config frame 和 devd WiFi endpoint 必须 redaction password/PSK。
- runtime config frame 和 devd runtime endpoint 必须能更新目标温度、主动散热开关与 heater hold 状态。
- devd 默认只监听 `127.0.0.1`，mutating endpoint 必须携带有效 lease。
- devd native serial discovery 必须只暴露当前明确授权的 MCU 端口；授权端口缺失时不得自动选择其它 `/dev/cu.*` 或 `/dev/tty.*` 设备。
- lease 必须有 heartbeat、TTL、过期 cleanup 和 conflict response。
- logs、trace、events 必须有固定上限。
- firmware artifact verify 必须校验 file existence、size 和 sha256；real flash 必须先通过 dry-run。
- devd artifact catalog 必须从本地构建产物计算 size/sha256，Web dry-check 必须调用 devd verify，而不是只做前端计时模拟。
- Web UI 必须在 capability 缺失、lease conflict、offline target、blocked artifact 时禁用危险操作并显示原因。

### SHOULD

- devd scan 应在授权端口范围内利用 serial USB metadata 构造稳定 ID。
- 后续 direct firmware HTTP 若落地，返回 shape 应与 devd bridge endpoint 共用 Web parser。
- Web app 验证应覆盖 nominal、devd unavailable、lease conflict、monitor/trace 与 firmware blocked/warning states；WiFi credential provisioning 属于 devd/USB contract，不作为 Web app 设置页面暴露，直到固件具备实际 WiFi station 连接能力。

## 接口契约（Interfaces & Contracts）

### HTTP / devd

- `GET /health`：返回 daemon identity、version、uptime、event/log/trace limits。
- `GET /api/v1/devices`：扫描并返回 known devices。
- `POST /api/v1/devices/:id/bind`、`connect`、`disconnect`：管理 daemon-local device record。
- `POST /api/v1/devices/:id/leases`：创建 lease；`POST /api/v1/leases/:lease_id/heartbeat` 续租；`DELETE /api/v1/leases/:lease_id` 释放。
- `GET /api/v1/devices/:id/identity|network|status`：读取同一领域契约；leased USB session 需要 `lease_id`。
- `GET /api/v1/devices/:id/events`：SSE 输出 bounded events。
- `PUT /api/v1/devices/:id/wifi`：通过 USB bridge 写 WiFi config；request/response 不回显 password。
- `PUT /api/v1/devices/:id/runtime`：通过 USB bridge 写运行时控制项；支持 `target_temp_c`、`active_cooling_enabled`、`heater_enabled` 的部分更新。
- `GET /api/v1/artifacts`：返回 daemon 可见的本地固件构建产物 catalog，包含 file kind、path、size、sha256 与可选 flash address；本地 ESP32-S3 release ELF 必须作为 `elf` artifact 走 `espflash flash`。
- `POST /api/v1/artifacts/verify`：校验 catalog/artifact 文件。
- `POST /api/v1/devices/:id/flash`：`dry_run=true` 只校验；`dry_run=false` 必须先有同 artifact 的通过记录。

### USB JSONL

- `hello`：device 主动或 host 请求；返回 protocol、framing、identity、capabilities。
- `request`：`request_id` + `op`，支持 `get_identity`、`get_status`、`get_network`、`set_log_level`。
- `wifi_config`：`request_id` + `op=set|clear` + credential fields；response 只包含 redacted summary。
- `runtime_config`：`request_id` + runtime fields；response 返回更新后的 status。
- `response`：回显 `request_id`，返回 result 或 error。
- `status` / `log` / `error`：device-origin async frame。

## 验收标准（Acceptance Criteria）

- Given 无硬件环境，When 运行 host tests，Then USB frame parsing、request ID matching、redaction、runtime config、status adapter、lease expiry、bounded buffer、artifact verify 与 serial authorization guard 均通过。
- Given devd mock target，When 创建 lease 并 heartbeat，Then lease 未过期前 mutating endpoint 成功，过期后返回 conflict/expired error。
- Given artifact hash 不匹配，When 调用 verify 或 flash dry-run，Then 操作被阻断且 error 不泄露无关 host path。
- Given Web Update 页，When 运行 dry-check，Then 浏览器必须调用 devd artifact catalog/verify endpoint，并展示 daemon 返回的校验结果。
- Given Web app，When 打开真实控制面页面，Then nominal、devd unavailable、lease conflict、monitor trace、firmware blocked/warning 状态都可见。
- Given PR 收敛，When checks 完成，Then firmware、devd、Web build/test、Web app browser smoke 与授权端口硬件 smoke 均通过；WiFi provisioning 真机写入只通过 devd/USB smoke 覆盖临时 SSID set、clear、redacted event 和最终 disabled readback。

## 非功能性验收 / 质量门槛

- `cargo fmt --manifest-path firmware/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path firmware/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path firmware/Cargo.toml`
- `cargo test --manifest-path tools/flux-purr-devd/Cargo.toml`
- `bun run --cwd web check`
- `bun run --cwd web typecheck`
- `bun run --cwd web build`
- Web app browser smoke against Vite preview with `devd` running.

## Visual Evidence

- 证据来源：Web app runtime。
- `assets/web-app-devd-no-authorized-serial.png`：Vite Web App 连接当前租约 `devd`；授权端口缺失时只显示 daemon mock target，并在 trace 中标明没有授权 native serial target。
- `assets/web-app-devd-artifact-dry-check.png`：Vite Web App Update 页通过 `devd` 校验本地 ESP32-S3 固件产物，dry-check 返回通过。
- Chrome DevTools a11y snapshot on lease-managed `127.0.0.1:32082` against CORS-enabled `devd` `127.0.0.1:32083` verified the live Web page selects `USB JTAG/serial debug unit / DEVD` before daemon mock devices, reaches `LEASE ACTIVE`, displays real hardware PD/status values without mock simulation drift, shows WiFi state `DISABLED`, and includes bounded WiFi set/clear events in Runtime trace.

## 参考（References）

- `docs/solutions/device-control/web-native-wifi-bridge-console.md`
- `docs/specs/hhwq8-web-control-plane-demo/SPEC.md`
- `docs/interfaces/http-api.md`
