# Flux Purr 真实控制平面运行时历史（#m8r4q）

## 2026-05-23

- 创建真实控制面 topic spec，把 PR #27 solution 从架构建议提升为 Flux Purr 的可实现 contract。
- 决策：本轮无真机时不阻塞 merge-ready；必须以 host tests、mock serial、devd dry-run 和 Web app 证据覆盖可验证部分。
- 决策：`#hhwq8` 继续代表轻量 Web demo；真实 transport work 由本 spec 承接，避免把 demo spec 扩张成全量后台。
- 主工作区真机 smoke 完成：`mcu-agentd` flash 成功，reset monitor 观察到 frontpanel app runtime、CH224Q/PPS、heater `pps-mos` backend 与 dashboard loop 稳定输出。
- 真机 smoke 发现并修复 `devd` 两个缺口：artifact verify 失败不再允许 dry-run 通过，`--help` 不再误启动 daemon。
- `devd` real flash 边界要求真实写入绑定 lease 对应的 native serial port，空 artifact manifest 不再通过 dry-run；Web 控制台默认从本机 `devd` discovery 合入 live targets。
- Web runtime 控制接入 `devd` lease 后的 identity/network/status 与 runtime update endpoint；固件 USB JSONL 支持 `runtime_config`，可写目标温度、主动散热与 heater hold。
- `devd` native serial discovery 收紧为当前授权 MCU 端口，授权端口缺失时清除 stale native serial device 与 lease，避免 Web 自动连接蓝牙、debug console 或其它未授权串口。
- `devd` 提供本地 artifact catalog，Web Update dry-check 改为调用 `GET /api/v1/artifacts` 与 `POST /api/v1/artifacts/verify`；development CORS 允许 Vite JSON preflight，浏览器可直接验证本地 ESP32-S3 build output。
- 固件默认 release artifact 纳入 `web_serial` feature，避免 `mcu-agentd` 默认烧录路径产出不响应 Web/devd 控制面的镜像。
- `devd` native serial RPC 失败会把设备标记为 `connection=error`，保留 `network.state=timeout/error` 与 serial event，避免 Web 把已枚举但未响应的授权端口误判为可控硬件。
- 固件 USB JSONL response 改为有界 chunk flush，避免 identity/status 等大于 USB Serial/JTAG 64-byte FIFO 的 JSON 帧被逐字节写入路径截断，同时避免无界阻塞启动。
- 固件启动期在完整 frontpanel runtime 主循环就绪前轮询 USB JSONL，允许 host 在显示、PD、EEPROM 或传感器初始化窗口内读取 identity/network；runtime status 与写命令在启动期返回可重试 `startup_busy`。
- `devd` serial bridge 对 firmware `startup_busy` 响应执行 bounded retry，并只对只读请求启用无响应重发，避免刚复位或 USB/JTAG 尚未初始化时把 Web 状态读取直接变成失败，同时不对写命令做静默重复提交。
- Web live devd bridge 将 daemon bounded events 转成 Runtime trace 条目，monitor 面板可以展示 serial/lease/flash 事件的安全摘要。
- `devd` flash route 现在为 dry-run、real flash blocked/started/completed/failed 写入 bounded events，并记录 selected artifact，让 Web trace 能看到更新链路状态。
- `devd` lease release 现在会写入 bounded device event，让 Web trace 能看到 native serial 控制权释放边界。

## 2026-05-25

- 移除固件对 `esp-println` / `esp-backtrace` 的依赖，改由本地 panic handler、no-op `defmt` logger 与 `esp-hal` `UsbSerialJtag` driver 支撑 USB JSONL 控制面。
- 修复移除旧 logging stack 后暴露的 `embassy-executor` pre-main `task arena is full` panic，固件显式使用 `task-arena-size-32768`。
- 授权端口 `/dev/cu.usbmodem21221401` 上完成真实硬件闭环：direct USB JSONL `hello` / `get_identity` 成功，`devd` hardware smoke 覆盖 identity、network、status、artifact dry-run、runtime mutation/readback/restore 与 lease event stream。
- 修复 native `devd` runtime 成功路径持锁 emit event 的死锁；硬件 smoke 在 runtime readback 前 heartbeat lease 并等待固件持久化 debounce，避免 macOS 重开 USB Serial/JTAG 触发 reset 后读到旧配置。
- `devd` WiFi 与 runtime 成功写入现在会写入 bounded device event；WiFi event 只记录 op、SSID 与密码是否存在，不记录密码本体。
- Web/devd 真实烧录路径改为对本地 ESP32-S3 ELF 使用 `espflash flash --after hard-reset`；raw `write-bin` 仅保留给带 explicit flash address 的 app binary，避免把 ELF 当裸 binary 写入 app 分区。
- 固件运行期 `runtime_config` USB response 从 ack 对齐为更新后的 `status` payload，避免 host 只能依赖后续 status 轮询证明 runtime 写入生效。
- `devd` runtime bridge 直接解析 `runtime_config` response 内的 `status` payload 并更新 device record，减少写操作后的额外 USB request 和超时面。
- 固件与 artifact catalog 不再声明尚未实现的 direct `net_http` / HTTP events capability；当前硬件控制路径以 `devd` + USB JSONL 为准。

## 2026-05-26

- Web live devd scenario 默认目标选择改为优先 active/native `devd` 设备，不再依赖 daemon 返回顺序；Playwright e2e 覆盖 daemon mock 在前、native serial 在后的回归场景。
- Web Dashboard 对 live `devd` 设备显示 daemon/firmware status，不再套用 mock 温度仿真或乐观 runtime 覆盖；Playwright e2e 覆盖 live devd 数值在刷新窗口内不漂移。
- 修复 native `devd` WiFi 成功路径持锁 emit event 的死锁；WiFi set/clear/restore 真机 smoke 可通过同一授权 USB JSONL 链路完成。
- `scripts/devd-hardware-smoke.py` 在长 smoke 阶段之间 heartbeat lease，避免 artifact、WiFi 或 runtime 读回步骤误用过期 lease。
- `devd` native serial RPC 增加 port-scoped process lock，避免多个 daemon 进程同时打开同一 USB Serial/JTAG port 导致 `Broken pipe` 或短时断线。
- 授权端口 `/dev/cu.usbmodem21221401` 上完成 Web -> `devd` -> USB JSONL -> firmware 浏览器验证：Web 自动选中 `USB JTAG/serial debug unit / DEVD`，达到 active lease，读取真实 PD/status/network，并通过 active lease 执行 runtime 写入。
- 授权端口 WiFi provisioning 复验完成：临时 SSID set、clear、restore 与 redacted bounded events 通过 smoke；最终直接 USB JSONL clear 后 `get_network` 返回 `state=disabled`、`ssid=null`。
- `devd` native serial RPC 改为复用持久 per-port session，并让 port-scoped process lock 跟打开的 fd 同生命周期，避免 Web/devd polling 每轮重新打开 ESP32-S3 USB Serial/JTAG 造成持续 reset；硬件验证显示首次 open 仍可能 reset，但后续 API/Web polling 和安全 runtime 写入期间 uptime 单调增加。
- Web runtime target control 改为在 devd/firmware 确认 `PUT /runtime` 成功后立即回显目标温度，并在下一轮真实 polling 对齐后清理临时覆盖，减少 live 硬件控制时的回显等待。
- Web Settings fan policy segmented control 改为在 devd runtime 写入成功后立即回显 operator 选择，避免按钮组选中态与反馈文本分裂；当前 firmware status 的 `fanDisplayState` 仍代表实际风扇显示状态。

## 2026-05-28

- 目标选择器的新增设备入口收敛为下拉底部唯一的 `Add device` 选项，不再使用下拉外的独立 USB 连接按钮，也不把 WiFi、Web Serial 与 Bridge 三种类型直接展开在目标下拉里。
- `Add device` 会进入单独页面，页面内提供 WiFi、Web Serial 与 Bridge 三种新增类型；Web Serial 类型在 live 模式继续作为 `navigator.serial.requestPort()` 的显式用户动作，demo 模式只创建待绑定预览目标，不触发真实后端或浏览器串口请求。

## 2026-05-29

- 决策：live 模式未选中真实目标时，不再展示 Dashboard、Settings、Update 或右侧全局日志列，而是展示全宽设备选择页，避免无目标状态混入空运行面板或 demo trace。
- 设备选择页按 known devices 网格、分隔线、单行 WiFi/Web Serial/Bridge 新增卡片组织；空设备提示不做成卡片，不显示额外分区标题；快捷新增入口先进入 Add device 页面再触发对应新增动作。
