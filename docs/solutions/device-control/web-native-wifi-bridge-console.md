---
title: Web、native USB 桥、烧录、监控与 WiFi 控制平面
module: device-control
problem_type: architecture
component: web-native-firmware
tags:
  - web-ui
  - native-daemon
  - usb-serial
  - firmware-flash
  - wifi
  - monitoring
status: active
related_specs:
  - docs/specs/hhwq8-web-control-plane-demo/SPEC.md
---

# Web、native USB 桥、烧录、监控与 WiFi 控制平面

## 背景

`IvanLi-CN/mains-aegis` 实现了一个面向 ESP32-S3 UPS 设备的浏览器管理控制台。它最值得复用的经验不是某个单页或某条 API，而是一套分层控制平面：

- Web UI 负责 fleet 导航、设备状态、设置 UX、固件选择和操作者防误触约束。
- native daemon 负责浏览器无法安全或稳定完成的主机能力：USB 串口发现、长时间 monitor session、本地固件文件访问、`espflash`、defmt 解码和 host power 操作。
- 固件通过 WiFi HTTP 与 USB CDC JSON lines 暴露同一套设备契约，让 Web app 可以跨 transport 复用同一个领域模型。
- WiFi provisioning 从 USB 开始，把凭据持久化到设备端存储，再在设备连接 WiFi 后切换到网络 HTTP。

这对 Flux Purr 的价值在于：如果后续需要一个能管理真实硬件的 Web console，同时又要支持离线 mock、浏览器 demo 和 native 工具能力，这套架构可以作为直接参考。

## 来源证据

参考仓库：`https://github.com/IvanLi-CN/mains-aegis`

关键实现证据：

- `web/src/app/App.tsx`：单一 app shell，包含 fleet、connect、overview、power、battery、thermal、device、firmware、settings、API 等页面分区。
- `web/src/api/client.ts`：统一 API client，支持 same-origin HTTP、显式 device base URL、`mock:*` 和 devd bridge endpoints。
- `web/src/api/statusStream.ts`：通过 `/api/v1/status` 建立 SSE 订阅，处理 status 与 heartbeat 事件。
- `web/src/serial/transport.ts`：Web Serial transport，使用 JSONL frame、request ID、响应超时、trace 捕获和可选 defmt decode。
- `web/src/app/firmware-page.tsx`：固件 catalog UI，支持 Web Serial 与 devd 两条烧录路径，包含确认门禁、进度状态和烧录期间锁定 drawer。
- `tools/mains-aegis-devd/src/main.rs`：Axum daemon，提供设备 scan、bind/connect/disconnect、Web USB lease、monitor、flash、WiFi config、settings、events 和 host power routes。
- `firmware/src/main.rs`：固件主循环发布 telemetry，处理 USB CDC 请求，持久化 WiFi 凭据，并在启用 `net_http` 时启动 WiFi/HTTP。
- `firmware/src/net_bridge.rs`：把内部 UI/power snapshot 适配成稳定的网络 status contract。
- `firmware/Cargo.toml`：通过 feature 拆分 `net_http` 与 `web_serial`；网络栈使用 Embassy、esp-radio、smoltcp。

## 目标

目标是构建一套硬件管理体验，能够：

- 通过 HTTP/WiFi、USB serial 和 mock/demo source 展示当前设备健康状态。
- 在 Web Serial 不足或不可用时，让浏览器连接 native USB daemon。
- 通过浏览器侧 Web Serial image 或 daemon 侧 `espflash` + 本地 artifact 完成固件烧录。
- 在不阻塞常规 UI 导航的情况下监控串口日志、协议 trace 和 status snapshot。
- 通过 USB 配置 WiFi，等待设备真实连接，再用同一套 identity/status contract 切换到 HTTP。
- 把危险操作放在显式状态、lease、确认、dry-run 和 capability check 后面。

## 架构方案

### Transport 无关的 Web 领域模型

Web 侧先定义稳定的数据模型，用来隐藏不同 transport 的差异：

- `DeviceTarget`：`deviceId`、`baseUrl`、alias/location、transport kind（`http`、`serial`、`devd`、`mock`）。
- `Identity`：固件版本、build ID、git SHA、feature list、设备 hostname、API version 和 capabilities。
- `NetworkSummary`：WiFi state、IP、gateway、DNS、RSSI 和 last error。
- `Status`：按用户可理解的硬件域分组，不直接暴露底层 driver 输出。
- `SerialSession`：连接状态、protocol version、当前 status、logs、trace 和 safe settings。

Web app 应该使用类似 `probeDevice(baseUrl, leaseId?)` 的 helper；由连接层决定 `baseUrl` 是 direct HTTP、same-origin devd 还是 mock。页面组件只关心领域状态，不把 serial/daemon 细节散落到 view code。

### Web UI

Mains Aegis 使用左侧导航 shell，并把 fleet、connect、firmware、settings、device telemetry 拆成稳定页面。硬件管理工作流超过一个时，可以复用这种结构：

- Fleet page：扫描/比较设备，显示 severity 和 connection state。
- Connect page：添加 direct HTTP endpoint、Web Serial connection 或 native daemon device。
- Device pages：拆分 overview、power/thermal/device info、firmware、settings 和 API/debug。
- Firmware page：source-aware artifact list、基于 identity 的兼容性检查、显式确认、进度日志、烧录期间禁用关闭。
- Settings page：WiFi 与其他 safe settings，通过 device status 展示 inline progress phases。
- API/debug page：给开发者看 raw contract，不污染 operator overview。

对 Flux Purr 来说，mock seeds 与 Storybook/test routes 应作为一等输入。Web app 必须在未连接硬件前就能完整演示和测试。

### Native USB-HTTP Daemon

Daemon 应该是本地 HTTP 服务，而不是 UI 专用私有通道。Mains Aegis 使用 Axum，并把能力分为几组：

- 健康与兼容：`/health`、`/api/v1/ping`、`/api/v1/identity`、`/api/v1/network`、`/api/v1/status`。
- 设备生命周期：scan、bind、connect、disconnect、unbind、reset。
- 固件生命周期：select artifact、verify files、dry-run flash、real flash。
- 串口生命周期：create heartbeat lease、read session、stream events、start/stop monitor。
- 设置桥接：WiFi config、log level、safe manual preferences。
- 工具能力：defmt decode，以及可选 host power routes。

关键设计是 daemon-local `DeviceRecord` registry，加上有界 event/log/trace buffer。浏览器只看到 HTTP API；daemon 负责平台细节，例如 serial port enumeration、长驻 reader、subprocess 执行和本地文件路径。

### USB Lease 模型

Mains Aegis 用短生命周期 Web lease 保护 USB 控制权：

- 浏览器为某个 daemon device 创建 lease。
- lease heartbeat 维持 session。
- lease 过期后释放设备；如果没有其他 active lease，就断开对应设备连接。
- 会修改设备状态或读取 leased USB session 的 endpoint 必须携带 `lease_id`。

这个模型值得复用。它能避免两个浏览器 tab 同时对同一台 USB 设备修改 WiFi、log level 或 flash state。

### USB CDC 协议

USB serial 采用 newline-delimited JSON frame。建议最小协议能力如下：

- `hello` handshake：返回 protocol、framing、capabilities、identity 和 firmware build metadata。
- `request` frame：携带 `request_id` 和操作名，例如 `get_identity`、`get_status`、`set_log_level`、safe setting operations。
- `wifi_config` frame：支持 `set` 与 `clear`，SSID/PSK 只从 host 发往 device。
- `response` frame：回显 `request_id`，返回结构化 result。
- `status` frame：发送 telemetry snapshot。
- `log` 与可选 defmt frame：用于诊断。
- `error` frame：包含 code、message、retryable 和 details。

协议 guardrails：

- 所有需要回应的命令必须有 request ID 和 timeout。
- trace view 必须在展示或持久化前 redaction secret。
- 固件必须限制 line/frame buffer，并用 protocol error 响应非法输入，不能 panic。
- UI 是否启用 WiFi、settings、logs、flash，应由 capability flags 决定。

### Firmware HTTP 与 WiFi

固件侧应把一个内部 status snapshot 同时桥接给 USB 和 HTTP：

- 内部 power/UI runtime 构建 status snapshot。
- 通过小型 adapter 把内部 enum 与 sensor data 映射为稳定 API slug。
- network task 发布最新 snapshot，并提供 `/api/v1/identity`、`/api/v1/network`、`/api/v1/status`，按能力支持 SSE。
- WiFi credential 通过 USB 写入设备存储，再应用到 runtime network task。
- daemon/Web 等待 network state 到达 `connected` 或 `disabled`，不能在写入凭据 ack 后就假定成功。

这样能避免 firmware network code 直接依赖每个硬件 subsystem，也让 API 演进更容易测试。

## 里程碑拆分

### Milestone 1：Contracts 与 Mock Console

交付一个可用 `mock:*` devices 运行的浏览器控制台。

- 定义 `Identity`、`NetworkSummary`、`Status`、`DeviceTarget`、`SerialSession`、`FirmwareArtifact` 和 API error envelope。
- 基于 mock data 实现 Fleet、Connect、Device Overview、Settings、Firmware 和 API Debug 页面。
- 添加 mock status stream 与 seeded device records。
- 通过 Storybook 或稳定 preview routes 产出视觉/测试证据。

验收标准：

- 不需要真实硬件。
- UI 能展示多设备、offline/error state、WiFi progress state 和 firmware compatibility state。
- contract fixtures 可被 firmware/native tests 复用。

### Milestone 2：Firmware Snapshot 与 USB CDC

交付固件侧 JSONL serial contract。

- 添加 `web_serial` feature。
- 实现 `hello`、`get_identity`、`get_status`、logs、errors 和 request ID matching。
- 通过窄 adapter 把内部 runtime state 映射为稳定 `Status` contract。
- 添加 WiFi config frame parsing、持久化、acknowledgement 和 secret redaction。

验收标准：

- 浏览器 Web Serial 能 handshake，并请求 identity/status。
- 非法 frame 返回 protocol error。
- WiFi PSK 不会出现在 log 或 trace 中。

### Milestone 3：Native Daemon MVP

交付用于 USB discovery 与 serial session 的本地 HTTP daemon。

- 实现 device scan 和 stable serial ID。
- 实现 bind/connect/disconnect 与有界 event/log/trace storage。
- 实现 Web USB lease、heartbeat、TTL、cleanup 和 lease-required endpoints。
- 通过直接 serial 或 monitor command channel 代理 USB CDC identity/status/settings 请求。
- 为 monitor output 提供 SSE 或 long-poll events。

验收标准：

- Web UI 可以把 `devd` 作为 same-origin 或 configured base URL 使用。
- 多个 browser tabs 不能在没有 lease conflict handling 的情况下同时控制同一 USB 设备。
- monitor session 运行时，UI 仍能获取 status 与 logs。

### Milestone 4：WiFi Provisioning 与 HTTP Handoff

交付 USB 到 WiFi 的配置链路。

- 添加 firmware `net_http` feature 与 HTTP status/identity/network endpoints。
- 如果平台支持，添加 mDNS/DNS-SD；否则提供明确 hostname convention。
- daemon 通过 USB 发送 WiFi config，然后轮询 status，直到 network state 变为 `connected`。
- Web UI 在 WiFi provisioning 成功后，把设备转换为 direct HTTP target。

验收标准：

- WiFi 设置过程展示 `saving`、`connecting`、`connected`、`error` 和 `timeout`。
- Clear WiFi 会等待 `disabled`。
- UI 不会在 firmware 报告目标 network state 前显示成功。

### Milestone 5：Firmware Catalog 与烧录

交付受控 firmware update 路径。

- 生成 firmware catalog，包含 artifact ID、version、build ID、git SHA、profile、feature list、target chip、protocol、defmt metadata、file hashes 和 flash addresses。
- Web Serial 路径烧录浏览器可 fetch 的 image files。
- Daemon 路径校验本地 artifact files，先 dry-run，再对 bound port 调用 `espflash`。
- UI 要求确认 device/artifact，并在 write 运行期间锁定 flash UI。

验收标准：

- 不兼容 firmware 被阻断，或必须显式 override。
- real daemon flash 前 dry-run 成功。
- flash logs 暴露进度和失败原因，但不泄露无关 host 细节。

### Milestone 6：Monitoring 与 Diagnostics

交付不干扰正常 UX 的操作诊断能力。

- 添加 serial monitor start/stop endpoints。
- 为每台设备保存有界 trace/log buffer。
- artifact metadata 匹配时解码 structured logs/defmt frames。
- 添加 API Debug 和 trace views，并与 overview pages 分离。

验收标准：

- 操作者能查看最近 frames 和 logs。
- 开发者能诊断 protocol failure。
- Overview 仍保持可读，不变成 raw telemetry dump。

## Guardrails

- 把 HTTP/WiFi、Web Serial、native daemon 和 mock 视为同一 contract 的不同 transport，不要做成四套产品。
- native daemon 的 mutating actions 必须放在显式 lease 和 confirmation gate 后面。
- 进度状态优先来自设备 status，不要用 optimistic UI completion 代替真实硬件结果。
- firmware adapter 要窄：内部 power/runtime state 不应把不稳定 enum name 泄漏到 API contract。
- 使用 feature flags（`net_http`、`web_serial`）在 firmware identity 中明确 transport 能力。
- WiFi PSK 和其他 secret 必须在 logs、traces、errors、UI history 中 redaction。
- 所有 log、trace、event 和 serial line buffer 都必须有上限。
- 烧录路径要 source-aware：浏览器可以烧录 remote image artifact；daemon 只能烧录本地磁盘存在的 artifact。

## Flux Purr 复用建议

Flux Purr 已经有 Web surface 与 ESP32-S3 firmware 工作。最低风险采用路径是：

1. 先做 contract fixtures 与 mock UI，让 UX 和 API 在硬件 transport 前稳定下来。
2. 先加 USB CDC JSONL，再加 WiFi HTTP；USB 是恢复与 provisioning 路径。
3. 只有当 Web Serial 无法覆盖 discovery、flashing、monitoring 或 local artifact access 时，再加入 native devd。
4. 等 USB status contract 稳定到可复用后，再加 WiFi HTTP。
5. 等 artifact catalog 与 compatibility checks 存在后，再加 firmware flashing。

不要从一次性的 daemon flasher 开始。Daemon 应该作为所有浏览器无法完成的 native capability boundary；否则后续 monitoring 与 WiFi provisioning 会分裂成多套路径。

## Flux Purr Web Demo 承接

Flux Purr 的当前 Web demo surface 由 `docs/specs/hhwq8-web-control-plane-demo/SPEC.md` 冻结。该 demo 只实现 mock-first 的轻量热控 bench 工具界面：低频选择一个设备，查看 Dashboard 热板运行态，在 Settings 调整 preset / fan policy，并在 Update 中执行 firmware dry-check。它不代表 native daemon、USB CDC、WiFi HTTP 或真实 flashing 已经实现。

后续真实 transport work 应复用 demo 中的领域语言和状态分组，但不要把本 demo 扩张成 fleet 管理后台；接口契约仍应在对应 firmware/native/Web spec 中单独冻结。
