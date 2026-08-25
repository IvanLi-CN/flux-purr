# Flux Purr 真实控制平面运行时历史（#m8r4q）

- 热模型校准曾采用 source-independent 的双锚点流程。该历史记录保留用于解释 `0x36/0x37`；当前实现改用单次瞬态轨迹，旧记录仅可解码，不迁移为 active，也不解锁加热。

## 2026-08-25

- Live Web Serial 恢复曾沿用全局 `devd` 自动轮询；daemon 不可达时，这条直连路由会被错误带入 `live-devd-unavailable` 诊断上下文。路由现在先依据当前请求或已记住的 transport 隔离 discovery：Web Serial 保留原身份路由并等待 operator 的显式连接动作，未指定 transport 与 Bridge 继续使用既有 devd 行为。这样未完成 Web Serial identity probe 的浏览器状态不会升级为另一种传输，也不会把 daemon 占位伪装成目标设备。

## 2026-08-05

- Direct-LAN 写入曾在 `stale_write` 后只显示失败或只更新 runtime snapshot，导致 calibration 与 heater-curve 工作区保留过时事实；各类写入现在只回读一次对应资源并替换其 state map，拒绝原写入且禁止自动重放。DEVD LAN bridge 曾把设备 HTTP 错误改写为本地 bridge error；adapter 现在保留远端 status 与完整 error envelope，包括 unauthorized。CI 在 Storybook interaction 前准备 Chromium，EdgeOne 只消费成功 push 运行生成的 verified artifact。
- Add Device 的桥接页曾无条件渲染全局“最近操作”，导致后台 native target 轮询错误与当前桥接配置混在一起。桥接路径现只呈现面板自身的发现、选择和错误状态，LAN 候选确认也在面板内完成。
- `flux-purr-devd serve` 曾在省略 `--serial-port` 时依次读取环境变量、用户配置和硬编码具体端口，随后又把 `serial_port=None` 错误实现成不发现任何 USB，导致多设备环境只能看到既有 LAN 记录。无参数启动现在保持 `serial_port=None`，枚举全部符合项目 USB 身份规则的未验证 transport 候选，但不自动选择、打开或探测；只有显式 flag 或 operator 选择才确定 USB 目标。
- Runtime trace 的 live adapter 曾把事件反转为新到旧，而“跟随尾部”仍滚到数组末项，导致按钮实际追随最旧事件。所有来源现统一按旧到新进入 UI，operator actions 追加到末尾，摘要与虚拟滚动尾部共同指向最新事件。
- DEVD 为缺席授权串口生成的诊断占位曾被 Web 当成真实桥接 target，并把串口路径派生键标成设备 ID。transport adapter 现以显式可连接事实区分占位与真实设备；占位只保留 record ID 与 port path 用于诊断，固件身份字段保持为空，不进入设备卡片或连接方式集合。
- 浏览器现在记忆已成功确认的 Web Serial 设备身份，并把该连接方式保留在对应设备卡片中。设备卡片快速连接会优先复用唯一授权端口，但仍以固件 `deviceId` 重新验证；USB 设备已替换时，实际硬件会作为另一台设备注册，旧记录保持离线，不会因浏览器授权仍存在而误合并。
- Add device 的 Web Serial 卡片不再因已有浏览器串口会话而禁用，也不再把点击解释成“返回当前设备”。添加动作在用户手势内强制打开原生端口选择器，选中新端口后安全替换旧会话；取消选择时继续保留当前设备。
- 固件缺少 thermal model 时，`runtime_config` 曾先回包 `heaterEnabled=true`，随后状态轮询才回到安全锁定的 `false`，造成 Web 出现短暂成功或永久等待。响应现在在应用前归一化为设备事实，Web 的确认计时使用真实时间戳并在 2.5 秒内收敛到明确拒绝；浏览器点击加热不会触发复位或虚构输出。
- 浏览器 Web Serial 的首次设备选择必须在点击手势内同步调用 `requestPort`；授权端口查询移至稳定页面生命周期，确保复用已授权端口不会阻断首次授权。取消原生选择器、超时和安全锁状态分别归属明确的 UI 状态，不再相互覆盖。
- Web Serial 连接完成后，连接事务的自动选择标记曾持续存在，导致用户在同一设备卡片中点击 WiFi/LAN 后又被异步 Web Serial effect 立即切回。显式连接方式选择现在会结束旧 transport 的自动采用意图；已连接通道继续保留，但不得夺回当前 target。
- 已配对 LAN session 曾只持久化地址、token 与可选 hostname，页面刷新后必须等待在线 probe 才能重建设备卡片；设备暂时不可达时 WiFi/LAN 连接方式因此完全消失。成功 probe 现在补齐稳定设备身份摘要，恢复流程先显示离线 LAN 通道再异步刷新，普通 transport 失败不会删除已知连接方式。
- LAN bearer 请求曾在任意 `401` 时删除整个本地 session，把“授权失效”错误地当作“设备不存在”。本地记录现在分别保存设备身份与 authorization state；`401` 只停用旧凭据并要求重新配对，设备记忆持续保留，直到用户主动遗忘或同一地址确认已经更换为不同 device ID。
- live devd scenario 曾在每次状态重算时注入 `devd reachable` synthetic event，并用字符串 `live` 充当时间；切换连接方式因此反复看到同一条伪日志且时间列无时间。连接状态现只存在于 target 状态，Runtime trace 只保留真实事件并统一显示本地 `HH:mm:ss`。
- Browser Web Serial 曾把来源标识 `web` 写入时间列；事件追加边界现直接生成本地 `HH:mm:ss`，使浏览器直连与 daemon 事件使用相同的时间契约。
- live operator action 曾复用 demo 固定时钟，导致真实切换通道的 Runtime trace 显示伪造历史时间；live 路径现与 Browser Web Serial 一样在追加时生成本地 `HH:mm:ss`。Web Serial 身份记忆也不再因 firmware/build 元数据缺失而丢弃已确认的稳定 device ID。
- 已保存 LAN session 曾在匿名 health 后被忽略，页面每次重新要求四位码；连接现在先验证 health `deviceId` 与 session identity 一致，再恢复 bearer probe 和 lease。地址复用到不同设备时只删除旧 session，不向新设备发送旧 bearer。
- LAN lease 心跳的设备协议错误保留为可操作的页面反馈；只有浏览器未获得响应时才使用泛化 transport 结果，过期 lease 不得继续参与写入。
- Browser direct-LAN runtime 写入在旧 session revision 与设备状态竞争时，现会在 PUT 前读取最新 snapshot；若写入仍被 `stale_write` 拒绝，只回读一次并恢复 UI 到设备事实，绝不自动重放原控制命令或报告成功。

- 生产板 RTD 分压模型曾先后使用 `3000mV` 经验值和 `3300mV` 通用 3V3 假设。网表中的 `TPS62933` 反馈网络实际为 `31.6 kOhm / 10 kOhm`，按 `0.8V` 典型反馈参考得到 `3328mV` 设计名义值；运行时改用该值。约 `31°C` 的 PT1000 理论节点电压为 `1032.9mV`，若仍按 `3300mV` 反算会错误回报约 `34.58°C`。ADC 个体误差继续由持久化 RTD calibration 承担，分压供电名义值不作为 ADC 校准参数。
- Web Serial 连接曾把 `navigator.serial.requestPort()` 的无限等待直接映射为 `Web Serial (connecting)`，且点击后仍保留旧的 devd `Failed to fetch` 反馈。连接事务现以 `60s` 为总预算，端口打开后不写 DTR/RTS、不等待设备重启，直接执行有界只读 probe；开始时立即显示等待串口选择提示，超时后给出可重试错误并关闭迟到的端口。
- Browser Web Serial 曾使用 `2 KiB` 单行缓冲，而 firmware 与 native devd 的 USB JSONL 合同为 `8 KiB`。identity/network 能被读取，但完整 status 超过浏览器上限后被静默丢弃，最终表现为连接超时。浏览器 reader 现与协议统一为 `8 KiB`，测试用超过 `2 KiB` 的 status fixture 锁定该回归；真机已通过 identity/network/status 和 `runtime_config` 写入回读。
- Browser Web Serial 曾静默丢弃 firmware 的 reset/panic 标记，设备复位后页面只能留下旧状态或泛化断线。reader 现仅接受协议定义的 `reset_reason=` 与 `panic=` 标记，清除复位前 snapshot 并显示设备报告的原因；任意 boot chatter 仍不会改变 UI 状态。
- 顶部目标选择器曾把同一硬件的 native DEVD、Web Serial、WiFi/LAN 与 bridge records 当成多台设备，用户无法确认要连接的物理设备。现按固件稳定 `identityId/deviceId` 合并为单张设备卡片，卡片显示 hostname 与设备 ID，并固定提供最多三种公开连接方式：WiFi/LAN、Web Serial 与桥接。DEVD USB 与 WiFi/LAN 发现结果只作为桥接方式的内部来源；重复记录按健康状态优先，避免旧地址或异常记录覆盖当前 target。
- 设备卡片的三个连接方式与“添加设备”操作现在各自显示对应的 Lucide 图标，文字、次要地址和方向箭头保持原有布局，降低重复按钮的识别成本。
- 设备卡片曾用 `auto-fit` 把唯一连接方式拉伸为整行，顶部触发器也同时显示 CSS select 箭头与组件 Chevron。连接方式现固定为最多三列，单个方式保持三分之一宽度；触发器只保留一个随展开状态旋转的 Chevron。
- LAN pairing claim 曾生成缺少 `api` 字段结束引号的 `200` 响应，浏览器因此在领取 token 后抛出 JSON 解析异常；Web 又把该非协议化异常显示成对话框背后的通用连接失败。claim 响应现由 firmware host test 锁定合法字段边界，Web 将无法解析的成功响应归类为明确的协议错误，并在当前配对对话框内展示具体原因。
- Claim 响应修正后，Web 的并发 identity/network/status probe 暴露了设备只有一个 HTTP workspace 与单一 response signal 的限制。控制面使用两个静态 TCP socket 支持两个同时在途连接；identity/network 从发布快照直接回答，不消耗唯一的 mutation-capable workspace，status、SSE 与写入继续由主控制循环串行执行，并以单调 control revision 拒绝过时写。这个模型避免为常用只读 probe 扩增大 request buffer，也不把资源受限伪装成 PNA 或 WiFi station failure。Web 保持并发 probe，devd/CLI 在写前读取 revision，客户端不再以串行请求掩盖设备端限制。
- 设备重启后 LAN lease 会自然从设备内存消失，但已配对 bearer token 仍有效。direct LAN 在 operator 重新选择该连接方式时把 expired 重新申请为可控 lease；心跳失败本身保持 expired 和只读，只有设备明确返回 conflict 才保持冲突，避免静默抢占或把重启后的设备永久显示为 lease 失效。
- 原生 USB I/O 若被前一个请求占用，后续 devd 请求不能无限等待互斥锁。串口 worker 以有限等待返回 `serial_lock_timeout`，页面保持 transport 原因而不覆盖最后一次 WiFi 网络事实；物理连接恢复后可重新取得同一互斥会话。
- 真机 Chromium 配对显示 `200` 但 identity/network body 为空时，根因是 CORS、PNA、revision 与响应终止行总长超过 `384` 字节，heapless header 被静默截断。响应头现由纯 formatter 在固定 `640` 字节容量内完整构造并由 host test 验证，真机的跨域 bearer identity readback 已返回完整 JSON。
- 已配对 LAN target 从 Bridge 切回 WiFi/LAN 时曾持续显示禁用控件。lease effect 依赖完整的 `visibleDevice`，每次 DEVD poll 都会取消在途请求；现收敛为稳定连接字段，真机切回 WiFi/LAN 后重新取得 lease 并恢复控制。Web Serial 同时优先复用唯一的既有浏览器授权端口，避免不必要的选择器。
- CIDR 扫描候选曾显示“选择”，点击后只回填地址，用户还必须再次点击上方“连接设备”，造成候选行主动作与实际意图不一致。现改为候选行直接显示并执行“连接”，复用同一 health、配对、probe 和 lease 流程；连接未完成前仍不注册或切换顶部目标。
- 旧版固件在多个 bearer 读取同时到达时仍可能只接受一个 TCP 请求，导致 claim 已成功后 probe 被误报为“配对凭据已保存、读取状态中断”。固件现使用单一 TCP acceptor 和两个静态 socket；identity/network 走发布快照，唯一 mutation-capable workspace 承担 status、SSE 与写入。Web 仅对旧设备的传输级失败执行一次串行兼容重试，并把 lease 失败与 probe/配对失败分开表达，避免把成功配对事实覆盖成 WiFi 错误。
- 配对后的 control revision 曾只存在于内存 session；控制面随后从 localStorage 重新读取 session 时，第一次 runtime 写入会被 stale-write 保护拦截且没有设备请求。probe 现在把设备回报的 revision 持久化到同一 LAN session，刷新与自动恢复路径复用同一保护值。
- 冷启动恢复的 WiFi 配置不再复用运行时 `apply_wifi_config` signal。旧路径在 task spawn 前留下 signal，首个 DHCP lease 成功后会立即把 station 断开，同时状态机仍停在 `connected` 并永久等待下一次配置，表现为 USB 回显 IP/RSSI 正常但路由器 ARP 与 `/health` 均不可达。启动配置现由 `net::spawn` 直接初始化；运行时 USB 配置仍走独立 apply signal。断线等待同时改为保留 pending driver event，避免 DHCP 与监听注册之间的竞态。
- 授权端口 `/dev/cu.usbmodem2111401` 经 repo-local `flux-purr -> devd -> espflash` 重刷后保留 SSID `Ivan` 与九位密码长度。未再次提交 WiFi 表单的冷启动 trace 为 `connecting -> connected`，设备在 `192.168.31.189` 的匿名 `/health` 回显 `a0f262f20d6c / flux-purr-a0f262f20d6c`；该 receipt 证明 DHCP 后 HTTP 数据面可达，不替代尚未完成的 mDNS、配对、lease 与 Chromium 控制闭环。

## 2026-08-04

- DNS-SD 报文生成从 ESP 网络任务中抽为 host-testable `no_std` 模块；PTR 由错误的 cache-flush 独占记录修正为 shared `IN` record，SRV/TXT/A 保持 unique record 语义。桥接发现候选在配对、probe 和 LAN lease 完成前不再进入顶部目标列表，避免把“发现到地址”误报成“已连接设备”。
- LAN 扫描结果的后置样式覆盖曾把结果列表间距压为 `0`，并让选中行失去稳定的内容边界；现已恢复结果行间距、内边距和选中态边框，同时明确“选择候选”与“连接设备”是两个动作。成功完成 health、配对和 lease 后，顶部设备选择器才出现并自动选中 hostname。
- Web direct LAN 表单现在同时记忆设备地址和 CIDR 网段，并在面板重载时自动回填。此前只有 CIDR 偏好被恢复，设备地址会回到组件初始值，导致同一浏览器中重新进入面板时需要重复输入；两项记忆均只保存非敏感地址/网段，清空输入即删除，不保存密码、配对码或 bearer token。
- Web direct LAN 的设备地址、CIDR 扫描和四位配对码改用三个独立的原生表单。键盘回车必须与主按钮复用同一控制路径，确保校验、loading、错误处理和防重复提交一致；表单不嵌套，避免回车冒泡触发错误动作。
- LAN 扫描候选不再被视为已连接设备：选择候选只回填地址并保留顶部当前目标，只有匿名 health、配对、bearer probe 和 lease 全部完成后才注册稳定设备 ID。设备列表以固件 hostname 识别设备，IP 仅作为定位信息；这样可避免扫描结果、配对失败或租约冲突把不可控地址误报为在线目标。

## 2026-08-03

- Bridge 从“一次点击创建 pending device”收敛为两阶段显式选择：先选 DEVD 的 USB 或 WiFi/LAN 路径，再选具体候选设备。连接方式卡片本身不再产生目标，避免把 transport choice 伪装成已连接硬件。
- USB Bridge 候选曾以状态标签和底部“连接所选设备”表达操作，既无法说明正在识别哪一项，也可能让 USB 产品描述被误认为硬件身份。候选现在逐行提供连接按钮，连接过程通过 identity-first 模态框结算；协议或稳定身份不匹配统一视为未知设备并拒绝注册，只有 operator 确认成功结果后才进入设备列表。

## 2026-08-02

- Web direct LAN discovery 明确包含 operator 主动触发的受限私有 IPv4 CIDR scan。浏览器仅匿名探测公开 `/health`，不使用 mDNS、不后台扫描、不调用 `devd`；native discovery 保持独立，避免把浏览器直连和本机桥接混成同一程序路径。

## 2026-07-30

- ESP32-S3 LAN control-plane 的 Embassy executor 内存契约固定为 `task-arena-size-81920`。HTTP workspace 不进入 async task frame；两个 worker 使用静态 TCP 缓冲、一个静态 workspace 与一个回收堆 workspace，在支持 Web 与 DEVD/CLI 并发读的同时保留主栈及 WiFi/executor 内存余量。USB JSONL 在启动阶段持续可轮询，但固件不为主机重枚举设置阻塞恢复窗口；LAN 启动失败也不得阻断 USB 控制。

## 2026-07-27

- 纠正 CH224Q PPS APDO 选择合同：当多个 PPS APDO 都覆盖 `20V` 时，firmware 现在保持 capability 来自同一个 APDO，并按最大电流、最大电压、最小电压排序。此前宽范围 `3A` APDO 会先于 `20V/5A` APDO 被选中，导致 `auto` 100W source 错误解析为低电流 capability；新回归测试覆盖 `3.3~21V/3A + 5~20V/5A` 组合，要求回读 `5000..20000mV / 5000mA`。
- 使用授权 `/dev/cu.usbmodem2111401`、IsolaPurr `f293cc9c139e` / `http://192.168.31.224` 和已更换的 5A eMarked 线材，完成该修复的 real-flash 与短 HIL receipt。source 保持 `100W`、PD/PPS、`pd_pps_5a=true`、`pps3_limit_ma=5000`、`tps_mode=auto_follow`；设备 readback 为 `ppsCapabilityMaxMa=5000`、`currentMa=5000`、`thermalProfileResolvedBank=pps5a` 且无 manual PPS override。`100°C / 120s` 短测约 `30s` 到达 `99.27°C`，后续采样在约 `99.3~99.9°C`；未见传感器硬 fault、过温、runtime reset、source stale 或端口切换。结束回读已关热、开启主动冷却。该记录不是 profile 调优、EEPROM save 或 frozen/accepted baseline。
- 更正 `2026-05-25` 的 executor arena 历史：`task-arena-size-32768` 不能容纳当前 `flux-purr` 主任务的 `33,344`-byte allocation，仍会在 pre-main 阶段触发 `task arena is full` panic 并被 RTC WDT 重启。后续 LAN control-plane 的完整内存契约见 `2026-07-30` 条目。

## 2026-07-19

- `220°C` rerun3/rerun4/rerun5 的 current truth 已收口到同一高温点族。`2026-07-19` 的 rerun4 与 rerun5 都停在 `brake=701 / approachFloor=898 / damping=410 / lead=3 / holdEntry=159 / holdReheat=930`，失败形态也从早期的高侧 overshoot 收敛成低侧或低裕量：`missed_lower_band_before_limit`、`stable_window_broke_low` 与 `within_gate_low_margin`。这说明当前 `220°C` 剩余 blocker 已不是高侧余热，而是 low-side / low-margin plateau。
- 同日确认并修复了一处 host timing 缺陷：剩余 per-target budget 被错误折算进每轮 `stage_timeout_seconds`，导致 warmup/stage 在预算尾部被意外截短。当前 timing contract 改为每轮 active self-test 固定 `180s`，remaining budget 只裁剪冷却等待或阻止开始下一轮；warmup timeout 保持独立显式参数，不再由预算 slack 隐式生成。
- 同日将 IsolaPurr source 重新上电流程从 USB-C path 切换固化为 runtime output gate。真实掉电步骤为 `isolapurr power runtime output --enabled false`，用 `runtime.output_enabled=false` 和 USB-C 零电流/零功率或非 `ok` 状态证明不再出力；等待 `2s` 后执行 `isolapurr power runtime output --enabled true`，再确认 telemetry 推进、`auto_follow`、`100W`、PD/PPS 与 PPS 5A capability。旧的 `power output manual --usb-c-path disconnected` / `power output auto` 只改变 source path/mode，不再作为 thermal HIL 标准 power-cycle。
- 热失控告警合同收敛为两个 owner-facing 状态：`temp >= 420°C` 活动期间每 `1s` 播放一次热失控提示；回落到 `<420°C` 后若尚未确认，则以 `faultAttentionPending=true` 每 `10s` reminder。确认不得绕过活动热失控的绝对停热与提示。
- 删除分支中按相邻温度或 raw ADC 变化幅度升级 `sensor-glitch` fault 的启发式。PPS request/VIN transition 仍可触发立即重读，但有效重读继续进入既有温度采样链；只有 `SensorShort / SensorOpen / AdcReadFailed` 和绝对过温进入硬保护。
- 热失控未确认期间禁止 heater arm，并按现有主动降温包线强制风扇：`>60°C` 全速、`40~60°C` 为 `50%`；温度 `<40°C` 或收到确认时解除强制风扇，低于 `40°C` 本身不代替告警确认。

## 2026-07-17

- 主人因旧 bench source 异常，更换当前 100W HIL source 为 IsolaPurr `f293cc9c139e` / `http://192.168.31.224`。当时的 source-side stale / latched 输出恢复曾使用 USB-C path 切换；该历史流程已由 `2026-07-19` 的 runtime output gate 合同取代，不再作为 thermal HIL 标准重新上电路径。
- 完成 `100w / pps5a` 的三点 preliminary review bundle：`thermal-self-test-runs/preliminary-pd100w-pps5a-60-140-220-20260717/`。该 bundle 固定为 canonical HTML 形态，并显式标记 `bundleDisposition=preliminary_review`、`acceptedProfileRole=review_candidate_snapshot`。三个目标的单点 `60s` hold confirm 结果为：`60°C => overshoot 0.75 / p2p 1.71`、`140°C => overshoot 1.73 / p2p 2.94`、`220°C => overshoot 1.11 / p2p 1.59`。这份证据只代表当前审查候选，不代表 committed accepted baseline，也不代表 EEPROM saved bank。

## 2026-07-11

- 最终完整 ladder 在 60°C 首点捕获 RTD 原始读数突变：`rtdRawAdcMv` 在约 `0.4s` 内从 `985` 跳到 `1014`，旧 runtime 限幅把该阶跃摊成持续假升温并最终报告 `20.18°C` 假过冲。运行时已删除该温度拒绝路径，所有有效样本直接进入控制环；控制器同时把经实际/滤波温度共同确认的升温斜率与 `approachLeadTicks` 用于 warmup 提前交接，并受 `warmupReenterCentiC` 滞回约束；参数仍通过 API/EEPROM 控制。
- 固件状态温度不再从前面板 deci-Celsius 值反推；`boardTempCenti/currentTempC` 直接由内部 RTD 浮点测量值发布到 `0.01°C`。新固件在授权端口烧录后回读 `22.71°C / boardTempCenti=2271`，saved profile 与 `6100mV` working floor 同时完成重启恢复。

## 2026-07-10

- Thermal HIL source 前置收口为 IsolaPurr LAN 上的 `65W`、PD Fixed enabled、PPS enabled、`auto_follow` 与一次 USB-C `>5V` 实测；self-test 不再手动控制 TPS 或执行 port replug。
- 自测试判定修复为：首次在 10 秒内进入 hold 后锁存通过，不被后续 hold/approach 相位回摆覆盖；首次 hold 后连续 60 秒按真实墙钟计时并纳入全部温度样本，不因相位回摆暂停。
- PPS backend 在 heater armed 且控制输出为 `0%` 时保持当前 PPS 请求并关闭 MOS。旧固件在 140°C approach coast 从 `11.3V` 回到 `12V` 后出现 uptime reset；新固件真机复测未再出现该复位。
- firmware heater control loop 固定为 `10Hz`；每个 tick 聚合 `32` 次 RTD ADC conversion，并保留分数毫伏均值完成 calibration 与 PT1000 转换。默认 thermal filter alpha 统一为 `700`，参数继续由 API/EEPROM profile 覆盖。
- host 采样默认收口为 `300ms`，请求更慢值也 clamp 到 `300ms`。完整 status JSONL 约 `1.9KB`，该值提供标称 `3.33Hz`，避免 `5Hz` 饱和与 `333ms` 无调度余量两类失败。
- retune 增加高温近目标功率受限分类：当 `>=180°C`、approach 输出 `>=90%`、斜率 `<=1°C/s` 且未进入 Hold 时，候选直接收敛到 stable-band edge handoff 与近饱和 hold baseline。饱和 Hold 两侧振荡时，按实测振幅加宽 overshoot taper，避免 PPS 从近满功率跌到低压后再全功率回升。
- 正确 source 与授权端口上的最终稀疏 HIL 全部通过：60°C settle/overshoot/p2p 为 `7.534s / 1.2°C / 2.3°C`，140°C 为 `8.661s / 0.4°C / 2.3°C`，220°C 为 `0.905s / 1.1°C / 2.6°C`。三个 p2p 均为进入 Hold 后连续 `60s` 结果。
- 最终 profile 已通过 API 写入 EEPROM；同一授权端口重启后回读 `profileSource=saved`、preview=false、alpha `700`、working floor `6100mV` 与最终 220°C 参数，证明参数更新不依赖替换固件。

## HIL source identity correction

- IsolaPurr `856a141cdbd4` 的授权 LAN endpoint 是 `http://192.168.31.122`。历史中标注 `http://192.168.31.224` 的 run 实际连接到设备 `f293cc9c139e`，不属于本项目指定 bench source；这些条目保留用于追溯，但其 thermal 数值不得作为算法验收依据。
- 正确 source 上的 `60°C` HIL 证明旧 predictive coast 还被滞后滤波 gate 锁住：实际温度已进入 coast 区间、预测温度已越过目标时，控制器仍保持 approach floor。固件已移除该重复 gate，保留“预测越过 + 实际误差进入 point-level holdExit”的双重条件。
- thermal self-test 支持重复 `--candidate-profile-file` 的同目标批测，候选共享一次 source/lease，会在 `max(40°C, target-30°C)` 以下开始下一组并分别输出图表，批测不保存 EEPROM。真实批次 `thermal-batch-1783673723491-serial-303a-1001-d0-cf-13-08-a1-48` 完成 3/3 组且无 lease/source 错误，证明批内不切换供电可避免候选间掉电。
- IsolaPurr `manual-forced 20V` 会与 Flux Purr 启动时的 `12V` PD 请求冲突并触发 USB 重启，因此 thermal HIL source 默认改为 `auto_follow`；manual forced 仅保留给 source-only 测试。最后一次继续 HIL 前，授权端口 `/dev/cu.usbmodem21221401` 已有 `ls: No such file or directory` 且连续 30 秒未恢复的证据，未切换到其它 MCU 端口。

## 2026-07-09

- firmware runtime 温度拒跳路径已移除。真机 `220°C` 开发 HIL 中 `rtdRawAdcMv` 继续上升而 `currentTempC` 卡死不动的问题不再通过温度限幅或趋势故障处理；有效样本直接用于控制，保留的硬 RTD 故障仅为开路、短路、ADC 读失败与过温。
- 同日真实 flash 暴露出 host-side artifact 来源不一致的问题。现行构建与烧录合同只使用 `firmware/target/xtensa-esp32s3-none-elf/release/flux-purr`；默认 Cargo 输出不属于 `devd` catalog，不能作为 HIL 镜像。
- 使用正确 artifact 后，聚焦 `220°C` 的真实 HIL 仍未达标。最佳 run `thermal-1783608810927-serial-303a-1001-d0-cf-13-08-a1-48` 与 `thermal-1783609283479-serial-303a-1001-d0-cf-13-08-a1-48` 都得到 `maxOvershootC=1.0`、`holdPeakToPeakC=3.4`；把 `holdPower / holdReheat / holdExit / holdKp` 一起抬高的 run `thermal-1783609087433-serial-303a-1001-d0-cf-13-08-a1-48` 反而恶化到 `maxOvershootC=2.3`、`holdPeakToPeakC=4.7`。
- 去掉 `220°C` 点位 `approachLeadTicks` 的候选也没有形成可接受改进。run `thermal-1783609472418-serial-303a-1001-d0-cf-13-08-a1-48` 在 `132.3°C` 附近长时间平台，期间 `heaterOutputPercent=98`、`heaterPhysicalOutputPercent=100`、`pdContractMv≈17500`，但温度与 RTD 原始 ADC 都基本不再上升，说明当前高温功率请求映射在某些候选下仍会落入明显的供热平台。
- 这批 rerun 给出了当前最明确的开发结论：`220°C` 的“温度锁死”问题已经修掉，但当前混合控制器和高温功率映射还没有通过开发期 `220°C` 验收；剩余 blocker 是 `holdPeakToPeakC` 仍稳定高于 `3.0°C`，并且某些 near-target 候选会触发中温平台。

## 2026-07-08

- firmware approach 段的 near-target sustain floor 现已直接受 `holdReheatPowerPermille` 约束，而不是只在 `Hold -> Approach` 回升路径上才抬高到 reheat floor；同时 predictive coast 现在要求“实际误差 + 滤波误差”都已落入该温区 `holdExit` 守门范围，避免在仍显著低于目标时被预测项提前打成 `0%`。这一步直接针对 `140°C` 真机 HIL 中“接近目标后卡在 ~139°C 且 5 分钟超时”的证据。
- thermal self-test 的默认开发梯子已改为 `60 / 140 / 220°C`；`250°C` 保留给最终完整验收，不再作为开发期默认目标。
- 5A tuning 默认开发梯子已扩展为 full-batch：tuning anchors `60 / 100 / 140 / 180 / 220°C`，validation targets `80 / 120 / 160 / 240°C`。validation targets 用最终 review candidate profile 做独立 hold-confirm 验证，通过时只记录 `validation_passed`，不自动新增 tuning anchor。
- full-speed-to-stable 判定按 SPEC 恢复为 `±1.5°C`、连续 hold `10s` 的真实稳定窗口；首次进入 hold 不再直接算稳定。离线重放后 `140°C` 仍通过，旧 `60°C` 通过结论被撤销。
- candidate tuner 会把首次 hold 后仍继续上冲、且高侧显著大于低侧的形状判为残余热主导，优先增加 `brakeDistance / approachDamping / approachLead`，避免误入普通 hold ripple 分支继续抬高功率。
- thermal profile settings 增加 EEPROM/API 可控的 `heaterCurrentReserveMa`；heater safe-max 从 capability/live current 的较小值中扣除该余量，避免把 source 限流预算全部分配给加热器后造成板级复位。
- `ThermalControlProfile` 每点已扩展为 power baseline + damping 数据：`holdEntryCentiC`、`holdExitCentiC`、`holdOffCentiC`、`overshootCutoffCentiC`、`holdKpPermillePerC`、`holdKiPermillePerCTick` 与 `holdBlendTicks` 现已随 preview/save、CLI/devd、report 和 EEPROM 持久化链路贯通。
- thermal self-test 的保温验收窗口现已改为只累计固件真实 `heaterControlPhase=hold` 的驻留时间；host-side “接近目标” 样本不再提前进入 hold peak-to-peak 统计。
- firmware hold handoff 现会 preload integral，并把最后一次 approach 输出在 `holdBlendTicks` 内平滑 blend 到 hold PI 输出；积分钳位也改成按 hold baseline 与 Ki 推导的动态范围。
- EEPROM thermal profile 编码只写入实际已配置点位，而不是固定写满 10 个槽；持久化上限为六点。EEPROM active record 使用 `1024 bytes` 新双槽，并保留旧 `512 bytes` 双槽只读回退，确保完整校准、最长 Wi-Fi 凭据和六点 profile 可同时保存。
- 同日完成两轮授权端口 `/dev/cu.usbmodem21221401` + IsolaPurr LAN source `http://192.168.31.224` 真机 HIL。第一轮结果：`60°C p2p 3.9`、`100°C overshoot 5.7 / p2p 8.5`、`140°C p2p 6.9`、`220°C p2p 4.7` 失败。第二轮回调后结果：`60°C overshoot 3.8 / p2p 5.4`、`100°C overshoot 4.6 / p2p 11.1`、`140°C p2p 4.6`、`180°C p2p 6.3`、`220°C p2p 5.4` 失败。两轮都证明高温段接近目标前的塌功率问题已收口，但低中温和高温保温仍受热惯性影响， acceptance 尚未达到 `<=3.0°C`。
- `ThermalControlProfile` 点位扩展 `approachFloorPowerPermille`，用于显式表达各温区接近目标时允许保持的最小加热功率下限；该字段与现有 `targetTempC` / `brakeDistanceCentiC` / `approachPowerPermille` / `holdPowerPermille` 一起经 runtime_config、CLI/devd、report 与 EEPROM 持久化贯通。
- Firmware hold control 改为围绕 `holdPowerPermille` 基线的连续 PI 微调；旧的全局 `approachMinPowerRatioPermille` 不再主导新 profile 的 approach floor，仅保留旧 EEPROM profile 的 decode fallback。
- Real HIL 进一步证明 `approachMaxTicks=16` 在当前约 `20ms` 主循环下只提供亚秒级 approach 窗口，无法覆盖加热台的真实热惯性；self-test 候选 profile 已改为显式更长的 approach 窗口，并把低温/高温 `holdPowerPermille` 调整到更接近实测等效保温功率的温区曲线。
- `approachMaxTicks` 的 runtime / EEPROM 验证上限已从 `60` 放宽到 `255`，避免 host 侧把 approach 窗口硬封在约 `1.2s`，使长 approach profile 能通过同一套 API 持久化到设备。
- 真实 HIL 证明“一组全局接近目标功率比率”无法同时满足低温和高温：高温 timeout 问题可以通过提高近目标功率解决，但会把 `60~140°C` 的 overshoot / hold p2p 拉高；后续调参基线改为按温区显式建模 approach floor 与 hold baseline。
- 授权端口 `/dev/cu.usbmodem21221401` 与 IsolaPurr LAN source `http://192.168.31.224` 的完整真机 HIL 已跑完整个 `60 / 100 / 140 / 180 / 220 / 250°C` 阶梯，说明 saved-profile 写入、按 EEPROM 生效、失败后 cleanup 清除这条链路已经打通；同时也确认高温段“不足功率导致接近目标前明显掉速”的旧问题已被消除，`180 / 220 / 250°C` 都能进入 hold。
- 同一轮 HIL 也证明“只把各温区的 approach floor 和 hold baseline 拉开”仍然不够：Applied run 仍在 `60°C (p2p 3.9)`、`100°C (overshoot 6.8 / p2p 10.1)`、`180°C (p2p 5.0)`、`220°C (overshoot 3.7 / p2p 6.8)`、`250°C (p2p 4.4)` 上失败。后续实现必须继续把 near-target damping 做成温区相关的可持久化控制数据，而不是继续只拧一组全局 hold dynamics。
- 同日后续实现把 `approachLeadTicks / holdLeadTicks` 接入 firmware、CLI/devd、report 与 EEPROM-backed profile，并完成完整真机 run `thermal-1783512058672-serial-303a-1001-d0-cf-13-08-a1-48`。该 run 的过冲已全部压到 `<=3.0°C`，但 hold peak-to-peak 仍在 `100°C 5.8`、`140°C 3.5`、`180°C 5.7`、`220°C 4.7`、`250°C 3.7` 失败，证明当前剩余 blocker 已经收敛到保温波动，而不是过冲本身。
- 同日继续把 `holdReheatPowerPermille` 接入 firmware、control-plane、CLI/devd、report 与 EEPROM-backed profile，并把控制器回升路径改成 `Hold -> Approach` 时使用该温区 reheat floor。第一次 run `thermal-1783513996870-serial-303a-1001-d0-cf-13-08-a1-48` 因 IsolaPurr `GET /api/v1/ports` 单次 `curl 28` 超时而中断，随后 host side 为 IsolaPurr LAN 遥测加入有界重试。
- 重跑后的完整真机 run `thermal-1783514578510-serial-303a-1001-d0-cf-13-08-a1-48` 把 `60 / 100 / 140 / 180 / 220 / 250°C` 全部跑完；最大过冲分别为 `1.8 / 0.9 / 2.0 / 2.1 / 1.7 / 1.4°C`，已全部满足阈值，但 hold peak-to-peak 分别为 `3.4 / 3.2 / 4.6 / 5.7 / 3.5 / 5.1°C`，因此 acceptance 仍失败于保温波动。

## 2026-07-07

- Runtime contract 扩展 `thermalControlProfile`：status 回传 `thermalControlProfilePreview`，runtime_config 可 `preview` / `clear_preview` 控制 RAM preview，也可 `save` / `clear_saved` 管理 EEPROM-backed active profile。
- `flux-purr thermal profile preview|clear-preview|save|clear-saved` 和 `flux-purr thermal self-test` 接入 CLI/devd lease 路径；self-test 生成报告、样本和候选 profile，并把默认目标阶梯限制在 `50..250°C`，排除 `300°C`；candidate profile 不自动保存。
- Thermal self-test 的外部电源边界固定为 released IsolaPurr CLI 的 LAN URL 路径：真实 HIL 通过显式 `--source-url` 调用 `isolapurr power output manual --url <url> --voltage-mv 20000 --current-limit-ma 3250 --usb-c-path forced-on` 设置 bench source，不使用 IsolaPurr saved hardware 的 USB transport；缺少精确 Flux Purr 端口、IsolaPurr source URL 或 expected device id 时不运行真实阶梯测试；IsolaPurr status identity、命令退出码与 power config readback 都必须可靠一致。
- 授权端口 `/dev/cu.usbmodem21221401` 上继续完成两轮真实聚焦 HIL：先刷入带“actual hold-entry gate + constrained hold preload”的固件，再刷入加入 `predictive coast` 的固件。最新 run `thermal-1783566799182-serial-303a-1001-d0-cf-13-08-a1-48` 使用 IsolaPurr LAN source `http://192.168.31.122`，把 `100 / 180 / 220°C` 的最大过冲分别压到 `7.9 / 4.6 / 4.4°C`，相较上一轮 `thermal-1783565969037-serial-303a-1001-d0-cf-13-08-a1-48` 的 `9.5 / 6.5 / 5.1°C` 明显下降；但 `holdPeakToPeakC` 仍为 `7.5 / 3.8 / 4.1`，说明剩余 blocker 已经集中在“predictive coast 起得还不够早”，而不是 hold 入口继续继承 approach 输出。

## 2026-06-02

- 授权端口 `/dev/cu.usbmodem21221401` 上完成手动 PPS 真机验证：新固件烧录后，`flux-purr pd pps set --volts 10.4 --amps 2.50` 回显 `manualPpsEnabled=true`、`manualPpsMv=10400`、`manualPpsMa=2500`、`pdContractMv=10400`，IsolaPurr `isolapurr-01-wifi` USB-C 外部遥测读到 `10425mV`；清除覆盖后回到自动控制与约 `12.03V`。

## 2026-05-31

- Runtime contract 扩展手动 PPS 调试覆盖：status 回传 manual/capability/error，runtime_config 可设置或清除非持久化 `manualPpsEnabled` / `manualPpsMv` / `manualPpsMa`。
- `flux-purr pd pps set|clear`、devd HTTP bridge、browser Web Serial 与 Web Dashboard 高级 PPS 控制共用同一 runtime contract；无授权端口时 HIL 保持阻断。

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
- Browser Web Serial 曾在 `port.open()` 后主动写 DTR/RTS，并固定等待 10 秒，把 USB Serial/JTAG 复位误当成连接初始化。连接客户端现不再触碰控制线或等待预期复位，打开后直接执行有界只读 probe；复位只能由固件诊断行明确报告，设备连接不得改变运行状态。
- Web runtime target control 改为在 devd/firmware 确认 `PUT /runtime` 成功后立即回显目标温度，并在下一轮真实 polling 对齐后清理临时覆盖，减少 live 硬件控制时的回显等待。
- Web Settings fan policy segmented control 改为在 devd runtime 写入成功后立即回显 operator 选择，避免按钮组选中态与反馈文本分裂；当前 firmware status 的 `fanDisplayState` 仍代表实际风扇显示状态。

## 2026-05-28

- 目标选择器的新增设备入口收敛为下拉底部唯一的 `Add device` 选项，不再使用下拉外的独立 USB 连接按钮，也不把 WiFi、Web Serial 与 Bridge 三种类型直接展开在目标下拉里。
- `Add device` 会进入单独页面，页面内提供 WiFi、Web Serial 与 Bridge 三种新增类型；Web Serial 类型在 live 模式继续作为 `navigator.serial.requestPort()` 的显式用户动作，demo 模式只创建待绑定预览目标，不触发真实后端或浏览器串口请求。

## 2026-05-29

- 决策：live 模式未选中真实目标时，不再展示 Dashboard、Settings、Update 或右侧全局日志列，而是展示全宽设备选择页，避免无目标状态混入空运行面板或 demo trace。
- Web Dashboard target stepper 改为先本地快速回显，并在短窗口内合并连续点击后只提交最后一个 live runtime 目标值，连续点击不再被上一轮 devd/firmware response cadence 卡住。
- `devd` bounded events 的 ID 加入单调序列，避免同一毫秒内多条 transport/runtime event 被前端去重吞掉；native USB JSONL TX/RX 作为 redacted `transport` events 进入 Runtime trace。
- Runtime trace 增加 all/info/success/warning/danger 等级筛选，并能展开显示 redacted transport frame payload，保留完整 request ID、frame type 与 TX/RX 数据。
- Web Dashboard 将 `currentMa` 显示为 PD contract 卡片内的电流能力，避免在 Runtime 小条中被误读为实时负载电流。
- Web Settings 的 fan policy 只暴露 firmware runtime contract 可写的 OFF/AUTO 冷却策略；RUN 保持为固件回报的风扇运行显示状态，不作为可写策略。
- 设备选择页按 known devices 网格、分隔线、单行 WiFi/Web Serial/Bridge 新增卡片组织；空设备提示不做成卡片，不显示额外分区标题；快捷新增入口先进入 Add device 页面再触发对应新增动作。
- 修复 pending Bridge/WiFi target 与 Web Serial 连接状态的选择同步：Web Serial 连接成功后必须选中真实 browser Web Serial target，不能继续显示 pending Bridge runtime。
- 将 preset 设置纳入真实 runtime contract：status 回传 `selectedPresetSlot` 与 `presetsC`，`runtime_config` 可写当前 slot 与完整 preset array；Web live Settings 以设备 status 为事实源，硬件前面板和 Web 同时显示 preset 设置界面时可通过写入 response 与轮询互相回显。

## 2026-05-30

- 决策：命令行正规控制面是 released `flux-purr` CLI，经 `flux-purr-devd` 操作 USB/flash/monitor 主机特权能力；浏览器 Web Serial 保留为浏览器访问硬件的正规路径。
- `flux-purr-devd` 启动形态使用 `serve` 子命令并默认绑定 `127.0.0.1:30080`；serial port 只接受显式 flag，无参数实例不选择 USB 设备。
- `flux-purr` CLI 覆盖 devices/identity/status/runtime/wifi/flash/monitor/hardware/usb-port，自动创建、heartbeat 和释放 lease，支持 human 输出与 `--json` 输出。
- 用户级硬件记忆写入 OS config directory，`FLUX_PURR_HOME` 可覆盖；daemon 不从该记录隐式选择 USB 端口。
- 发布收敛为单一 product tag `vX.Y.Z`；Web、firmware、host-tools 和 release manifest 挂同一 GitHub Release，manifest 的组件指纹决定是否需要升级。
- Repo 级 skill 分层现已拆为 developer policy 与 user/developer operations：`skills/flux-purr-developer-policy` 负责开发者总约束分流，`skills/flux-purr-user-operations` 与 `skills/flux-purr-developer-operations` 分别固化 released-user 路径与仓库内 developer operations/HIL 边界。
- Thermal profile persistence retains all ten point-local targets in each `pps3a` / `pps5a` bank by using the EEPROM v5 record layout, removing the six-point truncation from saved full-batch profiles; v1-v4 records remain readable through the compatibility decoder.

## 2026-08-04

- DEVD Bridge 的 WiFi/LAN 页面不再把 USB `/api/v1/devices` 空结果解释为没有 LAN 服务；它读取独立 LAN registry，并把 mDNS refresh 与 CIDR scan 暴露为 operator 显式动作。这样既保持 direct browser LAN 与 DEVD bridge 的所有权边界，也避免后台扫网。

## 2026-08-05

- 顶部 native target 名称改为固件 identity，而不是 USB product descriptor。选择器 trigger 与 dropdown item 分离渲染，避免 Radix 把双行下拉内容重复投影后造成文字重叠。
- 物理 WiFi Info 页面仍是四位 LAN pairing code 的唯一可用窗口，但已授权 USB/devd CLI 可以显式打开、读取并关闭该窗口。这样自动化能完成真实浏览器配对，同时不会把 code 或窗口控制暴露给 LAN。
- DEVD LAN bridge 将一次连接定义为 daemon 内完成的身份、网络和状态验证，而不是让 Web 在 verified response 后再次创建识别 lease。这样 bridge 初始化与后续控制 lease 形成明确边界，不会把同一浏览器的并发动作误报为冲突。
- LAN HTTP gate 曾只在入队时检查 lease，使排队写可能在 lease 过期后仍进入控制循环。mailbox 现在保留入队时的 lease ID，执行点再次验证 active 状态并以 `lease_expired` 结算过期命令。
- DEVD LAN bridge 曾把控制写局限于 daemon 本地记录，无法证明设备已写入。bridge 现在为每次远端写创建短期 device lease，以设备 status revision 完成 preflight 后转发真实请求，并在终态释放 lease。CI 随之覆盖 devd/CLI、Web unit、Storybook 和 E2E；EdgeOne 只部署成功 CI Main 的 verified Web artifact。
- USB Serial/JTAG 的连接不把 DTR/RTS 变更作为初始化步骤。Browser Web Serial 与 DEVD 都只打开端口并执行有界只读 probe，硬件复位只能由设备明确诊断上报。
- Browser Web Serial 的成功状态会结算先前的同通道失败反馈；“最近操作”不再能在已连接 target 旁保留过期的 `Web Serial unavailable`，也不会替换其它通道或运行操作的反馈。
- Web Serial 成功 probe 在 transport 层立即保存最小身份，而不是等待页面的后续 effect；已记忆通道在页面重挂载后仍与同一设备的 LAN/Bridge 通道合并。DEVD bootstrap/unavailable 不是控制目标，不会再向无关页面写入租约重连反馈。
- DEVD 设备列表刷新失败时，保留的不可用占位曾与旧的模拟 target ID 一起返回，使 live 页面误回退到添加设备页。占位现作为只读诊断当前目标保持 Dashboard 上下文；它持续禁用写入，且不会进入设备卡片或连接方式列表。
