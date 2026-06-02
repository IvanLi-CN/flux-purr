# Flux Purr 真实控制平面运行时（#m8r4q）

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，关键演进原因见 `./HISTORY.md`。

## 背景 / 问题陈述

- PR #27 已把 Web、native USB daemon、USB CDC、WiFi provisioning、firmware flashing 与 monitoring 的长期架构沉淀到 `docs/solutions/device-control/web-native-wifi-bridge-console.md`。
- `#hhwq8` 只冻结 mock-first Web demo，不代表真实 transport、daemon、USB CDC、WiFi HTTP 或真实 flashing 已交付。
- 本规范冻结 Flux Purr 真实控制面 v1：同一领域契约通过 mock、browser Web Serial、USB serial 和 native `devd` 暴露，Web 只根据能力启用操作；direct firmware HTTP 属于后续 `net_http` server 能力，不得在固件实现前声明。

## 目标 / 非目标

### Goals

- 定义 Web、firmware 与 `devd` 共享的 identity、network、status、USB JSONL、devd HTTP、firmware artifact 与 error envelope。
- firmware 提供 feature-gated `web_serial` contract adapter，复用现有热控 runtime 和 EEPROM WiFi 字段；direct `net_http` 只有在固件 HTTP server 落地后才可声明。
- `devd` 作为 localhost HTTP daemon，提供 USB/serial discovery、lease、monitor、WiFi bridge、artifact verify、dry-run 与 real flash command boundary。
- `flux-purr` 作为 released CLI，通过 `devd` 执行命令行硬件控制、用户级硬件记忆、USB 端口配置、artifact dry-run 与 guarded real flash。
- Release 使用单一产品 tag `vX.Y.Z`，Web、firmware 与 host-tools 资产挂同一 Release，并通过 release manifest 的组件指纹避免无效升级。
- Web demo 保持轻量 bench console 形态，但通过 client/transport 层接入 mock、browser Web Serial 与 devd contract，并对危险操作做 capability gate。
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
- `tools/flux-purr-devd/src/bin/flux-purr.rs` user CLI。
- `web/src/features/control-plane-demo/contracts.ts`、browser Web Serial client 与 transport client。
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
- runtime config frame 和 devd runtime endpoint 必须能更新目标温度、当前 preset slot、`presets_c[10]`、主动散热开关、heater hold 状态与调试用手动 PPS 覆盖。
- Web app 必须在目标下拉的底部只提供一个 `Add device` 入口；选择该入口后进入单独的 Add device 页面，并在该页面提供 WiFi、Web Serial 与 Bridge 三种新增类型。
- Web app 在 live 模式没有选中真实目标时，主工作区必须显示全宽设备选择页；该页上半部分显示 known devices 网格，空设备提示不得呈现为卡片，中间显示分隔线，下半部分以单行三卡片显示 WiFi、Web Serial 与 Bridge 三种新增类型，且不显示右侧全局日志列或额外的分区标题。
- Web app 必须通过 Add device 页面里的 Web Serial 类型提供显式 browser Web Serial 连接动作；连接成功后必须通过 USB JSONL 读取 identity、network、status，并用 `runtime_config` 直接控制目标温度、Settings preset slot / preset 温度 / preset 启用状态、主动散热开关与 heater hold 状态。
- Web app 的 live Settings preset UI 必须以 firmware/devd/Web Serial status 回读为事实源；当 firmware 前面板在 preset 设置界面修改 slot、温度或禁用状态时，Web 必须通过 live status polling 更新回显；当 Web 修改同一数据时，firmware 必须应用到前面板 UI state 并触发重绘。
- Web Serial 连接成功后，如果当前选中的是无目标或 WiFi/Web Serial/Bridge pending target，Web app 必须切换到真实 Web Serial target，不得继续显示 pending Bridge/WiFi runtime。
- Browser Web Serial 直连不得声明 firmware artifact verify、dry-run 或 real flash 能力；这些操作仍必须走 `devd` capability gate。
- devd 默认只监听 `127.0.0.1`，mutating endpoint 必须携带有效 lease。
- devd 必须以 `serve` 子命令启动，默认 bind `127.0.0.1:30080`，并保留环境变量兼容；flags 必须能覆盖 bind、serial port、artifact root、dev CORS 和 real flash。
- devd 必须在没有显式 `--serial-port` 或 `FLUX_PURR_DEVD_SERIAL_PORT` 时读取用户级默认 USB port；运行中变更默认 USB port 不得静默切换当前 daemon。
- devd native serial discovery 必须只暴露当前明确授权的 MCU 端口；授权端口缺失时不得自动选择其它 `/dev/cu.*` 或 `/dev/tty.*` 设备。
- `flux-purr` CLI 必须为 status/runtime/wifi/flash/monitor 操作自动创建、heartbeat 和释放 lease，支持 human 输出与 `--json` 输出，不要求用户手填 `leaseId`。
- `flux-purr pd pps set --volts <decimal> --amps <decimal> --device|--hardware` 与 `flux-purr pd pps clear --device|--hardware` 必须通过 lease 写 runtime contract；`--volts` 只接受 `0.1V` 步进且不高于 `21.0V`，`--amps` 只接受 `0.05A` 步进且不高于 source capability。
- `flux-purr hardware` 必须把 USB 设备记忆写入 OS 用户配置目录，`FLUX_PURR_HOME` 可覆盖；HTTP/LAN/mDNS 只能作为未来 transport 预留，不得伪装为当前能力。
- `flux-purr usb-port set` 必须写用户配置，并明确需要重启运行中的 `devd`。
- lease 必须有 heartbeat、TTL、过期 cleanup 和 conflict response。
- logs、trace、events 必须有固定上限；`devd` native USB JSONL TX/RX 必须作为 redacted `transport` events 进入 Runtime trace，保留 request ID、frame type 与 payload，WiFi password 等 secret 只能显示为 redacted。
- firmware artifact verify 必须校验 file existence、size 和 sha256，且只允许 artifact root 内的相对路径；real flash 必须先通过 dry-run。
- devd artifact catalog 必须从本地构建产物计算 size/sha256，Web dry-check 必须调用 devd verify，而不是只做前端计时模拟。
- Web UI 必须在 capability 缺失、lease conflict、offline target、blocked artifact 时禁用危险操作并显示原因。
- Web app 必须用 URL 参数 `demo=true|false` 选择 demo 或 live 版本，并在 browser storage 记住最近一次显式 URL 选择；缺少 URL 参数时必须回填记住的版本参数，不得在没有显式 URL 参数切换的情况下自动改变版本。
- `demo=true` 必须只加载 demo scenario，不得启用 devd、Web Serial 或任何真实后端请求。
- `demo=false` 必须使用独立 live scenario，不得混入 demo fixture、degraded demo 数据或 daemon mock devices；真实后端返回的 mock devices 也不得显示为 live target。
- Product release workflow 必须产出单一 `vX.Y.Z` 或 `vX.Y.Z-rc.<sha7>` tag，不得继续创建新的 `web/v...` 或 `fw/v...` tag。
- Product release manifest 必须记录 Web、firmware、host-tools 组件的 `sha256`、`contentSha256`、`sourceSha`、`protocolVersions`、`changedSincePrevious` 与 `updateReason`。

### SHOULD

- devd scan 应在授权端口范围内利用 serial USB metadata 构造稳定 ID。
- 后续 direct firmware HTTP 若落地，返回 shape 应与 devd bridge endpoint 共用 Web parser。
- Web app 验证应覆盖 nominal、devd unavailable、lease conflict、monitor/trace 与 firmware blocked/warning states；WiFi credential provisioning 属于 devd/USB contract，不作为 Web app 设置页面暴露，直到固件具备实际 WiFi station 连接能力。Add device 页面中的 WiFi 与 Bridge 类型在未绑定真实 transport 前只能展示待绑定状态，不得伪装为已连接硬件。

## 接口契约（Interfaces & Contracts）

### HTTP / devd

- `GET /health`：返回 daemon identity、version、uptime、event/log/trace limits。
- `GET /api/v1/devices`：扫描并返回 known devices。
- `POST /api/v1/devices/:id/bind`、`connect`、`disconnect`：管理 daemon-local device record。
- `POST /api/v1/devices/:id/leases`：创建 lease；`POST /api/v1/leases/:lease_id/heartbeat` 续租；`DELETE /api/v1/leases/:lease_id` 释放。
- `GET /api/v1/devices/:id/identity|network|status`：读取同一领域契约；leased USB session 需要 `lease_id`。
- `GET /api/v1/devices/:id/events`：SSE 输出 bounded events。
- `PUT /api/v1/devices/:id/wifi`：通过 USB bridge 写 WiFi config；request/response 不回显 password。
- `PUT /api/v1/devices/:id/runtime`：通过 USB bridge 写运行时控制项；支持 `target_temp_c`、`selected_preset_slot`、`presets_c`、`active_cooling_enabled`、`heater_enabled`、`manual_pps_enabled`、`manual_pps_mv`、`manual_pps_ma` 的部分更新。
- `GET /api/v1/artifacts`：返回 daemon 可见的本地固件构建产物 catalog，包含 file kind、path、size、sha256 与可选 flash address；本地 ESP32-S3 release ELF 必须作为 `elf` artifact 走 `espflash flash`。
- `POST /api/v1/artifacts/verify`：校验 catalog/artifact 文件。
- `POST /api/v1/devices/:id/flash`：`dry_run=true` 只校验；`dry_run=false` 必须先有同 artifact 的通过记录。

### CLI

- `flux-purr devices`：列出 `devd` 当前可见设备。
- `flux-purr identity --device <id>|--hardware <saved-id>`：通过 leased identity endpoint 读取设备身份。
- `flux-purr status --device <id>|--hardware <saved-id>`：通过 leased status endpoint 读取状态。
- `flux-purr runtime get|set`：读取或部分更新目标温度、preset、主动散热与 heater hold。
- `flux-purr pd pps set|clear`：设置或清除调试用手动 PPS 覆盖；设置路径要求 source status 已回报 PPS capability，且电压在 capability 与 `21.0V` 上限内，请求电流在 APDO current capability 内。
- `flux-purr wifi set|clear`：通过 leased WiFi endpoint 写入或清除 WiFi 配置，输出必须 redaction password。
- `flux-purr flash`：默认 dry-run；真实烧录必须显式 `--no-dry-run --confirm FLASH` 且 daemon 启用 real flash。
- `flux-purr monitor`：读取 bounded event backlog，不拥有长期未释放 lease。
- `flux-purr hardware available|recent|list|save|forget|path`：管理用户级 USB 硬件记忆。
- `flux-purr usb-port show|set`：查看或保存默认 USB serial port。

### Release manifest

- Product release tag 是 `vX.Y.Z` 或 `vX.Y.Z-rc.<sha7>`。
- Release assets 包含 Web bundle、firmware bundle、host-tools bundle 和 `flux-purr-release-manifest-<tag>.json`。
- `changedSincePrevious=false` 的组件不得被推荐为必需升级项。

### USB JSONL

- `hello`：device 主动或 host 请求；返回 protocol、framing、identity、capabilities。
- `request`：`request_id` + `op`，支持 `get_identity`、`get_status`、`get_network`、`set_log_level`。
- `wifi_config`：`request_id` + `op=set|clear` + credential fields；response 只包含 redacted summary。
- `runtime_config`：`request_id` + runtime fields；支持 `targetTempC`、`selectedPresetSlot`、`presetsC`、`activeCoolingEnabled`、`heaterEnabled`、`manualPpsEnabled`、`manualPpsMv`、`manualPpsMa`；response 返回更新后的 status。`manualPpsEnabled=false` 清除覆盖；启用时 `manualPpsMv` 必须在 PPS APDO capability 内、最高 `21.0V`、且按 `100mV` 对齐；`manualPpsMa` 必须在 APDO current capability 内、且按 `50mA` 对齐。CH224Q 只通过 `0x53` 写 PPS 电压，`manualPpsMa` 是用于校验与回显的请求电流值。
- `response`：回显 `request_id`，返回 result 或 error。
- `status` / `log` / `error`：device-origin async frame。

### Browser Web Serial

- Web app 使用 Add device 页面里的 Web Serial 类型调用浏览器 `navigator.serial.requestPort()`；未支持 Web Serial 的浏览器必须保持 mock/devd 路径可用并禁用 Web Serial 类型。
- Web Serial port 使用 `115200` baud 打开，按 USB JSONL 一行一帧写入 `request` / `runtime_config`，并只消费匹配 `requestId` 的 `response`。
- 直连 target 在 Web app 内标记为 `transport=serial`、`baseUrl=webserial://selected`、`leaseState=active`；该 active 表示浏览器持有当前 port，不等价于 `devd` lease。
- Direct Web Serial 控制项只包括 runtime control、manual PPS debug override 与 status polling；status polling 必须回读 target、preset、cooling、heater、manual PPS/capability/error 与 power/network summary，供 Web 与前面板设置界面双向回显。firmware recovery、artifact catalog、dry-run、real flash、daemon-local bind/connect/disconnect 不属于该直连通道。

## 验收标准（Acceptance Criteria）

- Given 无硬件环境，When 运行 host tests，Then USB frame parsing、request ID matching、redaction、runtime config、status adapter、lease expiry、bounded buffer、artifact verify 与 serial authorization guard 均通过。
- Given devd mock target，When 创建 lease 并 heartbeat，Then lease 未过期前 mutating endpoint 成功，过期后返回 conflict/expired error。
- Given artifact hash 不匹配，When 调用 verify 或 flash dry-run，Then 操作被阻断且 error 不泄露无关 host path。
- Given Web Update 页，When 运行 dry-check，Then 浏览器必须调用 devd artifact catalog/verify endpoint，并展示 daemon 返回的校验结果。
- Given Web app，When 打开真实控制面页面，Then nominal、devd unavailable、lease conflict、monitor trace、firmware blocked/warning 状态都可见，Runtime trace 支持按 info/success/warning/danger 等级过滤。
- Given 支持 Web Serial 的浏览器，When operator 在目标下拉底部选择 `Add device`、进入 Add device 页面选择 Web Serial 并选择 ESP32-S3 端口，Then Web app 通过 USB JSONL 读取 identity/network/status，并把目标温度、fan policy 与 heater hold 写为 `runtime_config`。
- Given Web Settings 与硬件前面板都显示 preset 设置界面，When operator 在 Web 选择 preset slot、修改 preset 温度或切换 enabled，Then firmware 通过 `runtime_config` 更新 `MemoryConfig` 与 `FrontPanelUiState`，前面板界面及时重绘，Web 从返回 status 或下一轮 polling 看到相同 `selectedPresetSlot` / `presetsC`。
- Given Web Settings 与硬件前面板都显示 preset 设置界面，When operator 在硬件侧切换 slot、调整温度或禁用 preset，Then firmware status 反映新的 `selectedPresetSlot` / `presetsC`，Web live Settings 在下一轮 polling 内更新回显，不再显示前端硬编码 preset。
- Given live 模式没有选中真实目标，When 打开 Dashboard、Settings 或 Update，Then 主工作区仍显示全宽设备选择页，不显示 Dashboard/Settings/Update 内容，不显示右侧全局日志列；WiFi、Web Serial 与 Bridge 三种新增卡片保持同一行，点击任一新增卡片进入 Add device 页面并触发对应新增动作。
- Given Add device 页面当前选中 pending Bridge，When operator 点击 Web Serial 并连接成功，Then 目标选择器显示真实 Web Serial target，Dashboard 显示真实 runtime，而不是继续显示 `Native bridge / BRIDGE`。
- Given Web Serial 直连 target，When 打开 Update 页，Then artifact verify、dry-run 与 real flash 仍因缺少 `flash` capability 被禁用或要求切换到 `devd`。
- Given CLI 指向 `devd` mock target，When 执行 devices/status/runtime/wifi/flash dry-run/monitor，Then CLI 自动 lease、输出可读 human 文本或 `--json`，且 secret 被 redaction。
- Given CLI 或 Web live target 具备 PPS capability，When operator 设置 `10.4V / 2.50A` 手动 PPS 覆盖，Then runtime status 回显 `manualPpsEnabled=true`、`manualPpsMv=10400`、`manualPpsMa=2500`、capability 范围和更新后的 PD request/contract；When 清除覆盖，Then status 回到自动 PPS 控制。
- Given 用户保存默认 USB port，When 重启 `flux-purr-devd serve` 且未显式传入 serial port，Then daemon 只扫描该用户配置 port。
- Given product release 发布，When 查看 release assets，Then Web、firmware、host-tools 与 release manifest 同挂一个 `vX.Y.Z` Release；manifest 可区分 unchanged component。
- Given PR 收敛，When checks 完成，Then firmware、devd/CLI、release policy、Web build/test、Web app browser smoke 与授权端口硬件 smoke 均通过；WiFi provisioning 真机写入只通过 devd/USB smoke 覆盖临时 SSID set、clear、redacted event 和最终 disabled readback。
- Given HIL 验收，When 主人提供并授权确切 USB 端口，Then 通过 `flux-purr` CLI 经 `devd` 证明 identity/status、runtime write/readback/restore、artifact verify/dry-run、real flash、重启后 identity/status/events；未授权端口时不得创建 ready PR。

## 非功能性验收 / 质量门槛

- `cargo fmt --manifest-path firmware/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path firmware/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path firmware/Cargo.toml`
- `cargo test --manifest-path tools/flux-purr-devd/Cargo.toml`
- `bash .github/scripts/test-release-labels.sh`
- `bash .github/scripts/test-version-scripts.sh`
- `bun run --cwd web check`
- `bun run --cwd web typecheck`
- `bun run --cwd web build`
- Web app browser smoke against Vite preview with `devd` running.

## Visual Evidence

- 证据来源：Web app runtime。
- `assets/web-app-devd-no-authorized-serial.png`：Vite Web App 连接当前租约 `devd`；授权端口缺失时只显示 daemon mock target，并在 trace 中标明没有授权 native serial target。
- `assets/web-app-devd-artifact-dry-check.png`：Vite Web App Update 页通过 `devd` 校验本地 ESP32-S3 固件产物，dry-check 返回通过。
- `assets/web-app-live-no-device-selection.png`：Vite Web App live `demo=false` 无真实目标状态显示全宽设备选择页；空设备提示以轻量文本呈现，单行三张新增卡片可见，右侧全局日志列和分区标题隐藏。
- `assets/web-app-live-preset-sync.png`：Storybook live Web Serial 场景覆盖 Settings preset 写入后从 status 回显；M5 被 Web 写为 disabled 后，summary、slot grid、selected editor 和 Runtime trace 保持一致。
- `assets/web-dashboard-manual-pps-request-current.png`：Vite Web App demo Dashboard 高级 PPS 面板显示两行 voltage/current request 控制、capability 动态范围、Apply/Clear 与请求电流说明。
PR: include
![Web Dashboard manual PPS current request](./assets/web-dashboard-manual-pps-request-current.png)
- Chrome DevTools a11y snapshot on lease-managed `127.0.0.1:32082` against CORS-enabled `devd` `127.0.0.1:32083` verified the live Web page selects `USB JTAG/serial debug unit / DEVD` before daemon mock devices, reaches `LEASE ACTIVE`, displays real hardware PD/status values without mock simulation drift, shows WiFi state `DISABLED`, and includes bounded WiFi set/clear events in Runtime trace.

## 参考（References）

- `docs/solutions/device-control/web-native-wifi-bridge-console.md`
- `docs/specs/hhwq8-web-control-plane-demo/SPEC.md`
- `docs/interfaces/http-api.md`
