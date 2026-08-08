# Flux Purr 真实控制平面运行时实现状态（#m8r4q）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、当前真相与剩余缺口，不保留会话级流水账。

## Current Status

- Implementation: Web + browser Web Serial + `devd` + CLI + USB JSONL + firmware `net_http` runtime 已覆盖 identity、network、status、runtime mutation、artifact verify、flash dry-run、real flash 与 monitor event 的真实传输路径
- Lifecycle: active
- Catalog note: direct firmware HTTP 默认随 ESP32-S3 runtime 构建；LAN 保持可信私网边界，初始 WiFi 配置、firmware flash 与 token reset 仍仅限 USB/`devd`
- Web live Settings 在 native `devd` target 具备 `wifi_config` 时显示 WiFi 表单。缺少 `wifi_state_v2` 时，表单保持可见但所有配置控件锁定并显示协议更新原因；具备该 capability 且持有 active USB lease 时才可提交。`PUT /wifi` 的 USB receipt 必须携带脱敏配置和设备已发布的 `NetworkSummary`；Web 不制造 `saving`、不设本地 WiFi 超时，也不把 transport 失败伪装为 WiFi 失败。`configurationGeneration` 与 `transitionSequence` 保护当前提交免受迟到旧包回退，只有同代或更新的设备 snapshot 可结算 loading。密码留在页面内存直到设备确认 `connected` 或 `disabled`，然后清空；重载只依据 `wifiPasswordLength` 形成等长掩码。
- WiFi 操作只存在一个顶部水平居中、固定定位的 Toast：设备 transaction 处于 `saving|connecting` 时显示 loading 和 `aria-busy`，设备发布 `connected|error|timeout` 或清除后的 `disabled` 时原位替换为终态，5 秒后自动关闭。保存或清除事务在设备终态到达前保持所有写操作按钮原生禁用，但只有触发事务的按钮显示对应 loading 图标与进行中文案，另一按钮保持普通文案。Toast 不占 WiFi 区域高度；提交校验和清除二次确认是唯一由本地表单直接产生的消息。自动重连由固件固定启用，不在 Web、devd、CLI 或 USB JSONL 中作为可配置项出现。direct LAN 与 Web Serial 不展示 provisioning 表单。
- Firmware LAN 在 route dispatch 的最前面固定 endpoint/method 映射；错误 method 在 bearer、lease 和控制邮箱之前返回 `405`。native serial 的 identity/network/status/calibration read 与四位 LAN pairing code 查询同样要求 active USB lease；CLI 自动取得、续租并释放该 lease，不向用户暴露 `lease_id`。
- `wifi_state_v2` 是独立于 HAL、Embassy、USB 和 UI 的 `no_std` 状态机，驱动 runtime network summary 的所有 state、generation、sequence 与有限 `failureCode`。配置断连最多 3 秒；每次配置事务最多三次尝试、总计 30 秒。可恢复的单次失败仍为 `connecting`，仅耗尽事务才发布一次 `error|timeout`。自动重连设备在终态至少保持 5 秒后才显式开始新的恢复过程，不能改写先前结算的 Toast。EEPROM 推导摘要仅用于 LAN runtime 尚未启动的 USB recovery 窗口。
- 固件区分冷启动配置装载与运行时 WiFi 重配：`net::spawn` 直接用 EEPROM/flash 配置初始化 runtime 和状态机，不触发 `WIFI_APPLY_SIGNAL`；USB `wifi_config` 才通过该 signal 请求断开并应用新配置。断线监听使用不清除 pending event 的 driver API，避免 DHCP 完成附近的断线竞态被遗漏后留下假 `connected`。
- `devd` 的 USB/serial bridge 失败只更新 device connection 与 transport event，不覆盖 WiFi network summary；Web 只在 WiFi `connected` 状态显示 RSSI，避免旧信号值与错误态同时出现。
- USB Serial/JTAG 打开导致设备重启时，固件启动早期的 `get_network` / `get_status` 只返回可重试的 `startup_busy`，不泄漏默认内存配置；devd 等待主循环完成 EEPROM/flash 恢复后再消费版本化 `NetworkSummary`，因此重连不会短暂显示 `disabled` 或空 SSID。
- 浏览器 LAN 配对已将“已领取稳定 token”和“已取得 device lease”分为两个事实：配对成功时只显示正在获取控制租约；只有设备确认 active lease 后才解锁 runtime 写操作。恢复的 LAN target 收到 `401` 时只删除该 target 的本地 token，不影响其它已保存设备。
- Firmware pairing claim 使用 host test 锁定完整 JSON 字段边界，确保 token 后的 `api`、`deviceId` 与 `hostname` 可以被标准 JSON parser 读取。Web LAN client 将 `2xx` 响应的 JSON 解析失败归类为 `lan_response_invalid`；required pairing 的 claim/probe 错误保留在配对对话框内，背景连接状态不再显示无关的通用失败。
- Web bearer probe 默认并发请求 identity、network、status；设备端由单一 TCP acceptor 把连接交给 3 个独立 HTTP worker，使用独立静态 workspace 与 response signal，避免多个 listener 争抢同一端口、连接 reset 或响应串线。设备读响应公开单调 `X-Flux-Purr-Revision`，Web session 合并最新值并用于后续 mutation；对仍只接受单连接的已发布旧固件，Web 仅在传输级失败时自动串行重试，认证和协议错误不会被重试或隐藏。claim 后的连接中断单独归类为 `lan_probe_unavailable`，不再显示成浏览器 PNA 拒绝。
- LAN probe 得到的最新 control revision 会回写到同一 origin 的已保存 session；刷新页面或重新选择已配对目标时，第一次写入不会因内存 session 丢失 revision 而被错误拒绝。写入仍必须携带设备最新 revision，过时 revision 继续由固件以 `stale_write` 拒绝。
- 浏览器 direct LAN 的候选与目标注册已经分离：CIDR `/health` 扫描结果的“连接”按钮直接复用 `health -> pairing -> probe -> lease` 流程，但在全部成功前不改变顶部目标；全部成功后才通过 `upsertLanDeviceTarget` 注册并选中 LAN target。注册按 `lan-<deviceId>` 稳定身份去重，DHCP 地址变化只更新原记录；设备选择器以 `identity.hostname` 为主名称，并把传输和 IP 放到次要行。配对、probe、lease 失败时保留原当前目标和列表。该链路使用注入式 LAN client fixture，Storybook 不启动 `devd`、mDNS 或真实设备。
- 扫描结果行使用独立的 sibling 间距、内边距和选中边框；候选行主动作明确显示“连接”，点击后立即进入匿名 health 和后续配对流程，连接进行中禁用候选行避免重复提交。Storybook 同时覆盖“扫描结果直接连接、配对并取得 lease”路径，成功后顶部选择器显示并自动选中固件 hostname。
- `LanPairingPanel` 的设备地址连接、CIDR 扫描和 required 配对码 claim 现在分别使用不嵌套的原生 `<form>`。主按钮与回车共用同一提交处理、校验、loading、错误和禁用路径；Storybook play 覆盖三条回车路径的单次请求、非法/不足四位输入和进行中防重复提交。CIDR 表单仍由浏览器直接执行匿名 `/health` 扫描，不调用 `devd`，与 native bridge discovery 保持边界。
- 浏览器 direct LAN 将设备地址和 CIDR 网段分别保存为当前 origin 的非敏感表单偏好（`flux-purr:lan-address`、`flux-purr:lan-scan-cidr`）。面板初始化时自动回填两项，operator 修改、扫描候选选择或连接提交时更新地址偏好；清空输入会删除对应记录。配对码、密码和 bearer token 不进入这两个键；本地偏好恢复不会自动连接或启动扫描。

## Coverage / rollout summary

- `web/e2e/control-plane-lan.spec.ts` 以独立 HTTP v1 fixture 覆盖 Chromium 配对、PNA/CORS、四位码终态、token recovery/401 purge、lease busy/expiry、SSE reconnect、runtime write/readback 和 Safari block；测试通过 `E2E_DISABLE_DEVD=1` 明确禁止启动或调用 `devd`。`devd` 的 native USB 测试仍单独验证自身 transport，不构成浏览器 LAN 的依赖。
- thermal profile persistence 使用固定 `pps3a` / `pps5a` 双 bank；每个 bank 最多持久化 `10` 个完整 point-local 目标点。EEPROM 当前写入 v5，使用固定 `2 KiB` active slot 与 `u16` TLV length，解码兼容 v1-v4，读取顺序为 `2 KiB active -> 1 KiB previous -> 512 B legacy`。旧单 profile 会在 RAM 中迁移到 `pps3a` 且默认 mode 为 `65w`，下一次成功配置提交时物化为 v5；EEPROM 启动不强制重写旧槽。
- runtime status、runtime config、CLI 与 self-test 已统一支持 `thermalProfileMode=auto|65w|100w` 与 `thermalProfileResolvedBank`。显式 `65w` / `100w` 为强制档；`auto` 仅按 source capability class 在 `pps3a` / `pps5a` 间解析，不按 live current 自动回退。
- `flux-purr thermal profile preview|save|clear-saved` 已是 bank-aware 路径。`preview` 仍是单一 RAM overlay；显式 `save` / `clear-saved` 会携带目标 bank，`auto` 下必须先应用 `thermalProfileMode=auto`，再从该请求的 status 回读 `thermalProfileResolvedBank`，最后才允许向 resolved bank 持久化。
- `flux-purr thermal self-test` 已支持 source-aware `auto|65w|100w`。65W 维持 `20V / 3.25A` 语义，100W 使用 `21V / 5A` 语义。报告与 HTML 已保留 `selectedMode`、`resolvedBank`、`detectedSourceClass`、source preset/readback 以及 per-stage `analysis.approachSource` / `analysis.holdSource`。
- self-test source capability power 现在可由 `--source-power-watts` 显式指定；host status readback 使用有界重试，并把 `status_request_failed` 作为可审计 stop reason，而不是无界卡死在单次 `/status` 失败上。
- `flux-purr thermal retune` 继续消费既有 `run.json` / `samples.ndjson`，并在 `--apply-preview` 下把 replayed candidate 与源 run 的 profile mode 作为 RAM-only preview 下发到目标设备。CLI 在 replay 产物已落盘后执行 preview，再通过 `/status` 校验 mode/bank、preview active、当前 target coverage、preview source 与逐字段 effective parameters，并把 apply receipt 追加到 `run.replayed.json`。
- canonical preliminary bundle 的 `samples.ndjson` 保留有效和无效 attempt 的全部原始样本，并在每条样本上写入 `evidenceValid` / `evidenceInvalidReason`；无效 evidence 只从评分与候选晋级中排除，不从审计产物删除。
- runtime status 现在显式回显 `faultAttentionPending`；runtime config、CLI `runtime set` 与 app live runtime 都支持 `faultAttentionAcknowledged=true`。attention 只属于热失控：`temp >= 420°C` 时每 `1s` 播放一次热失控提示，温度回落且未确认时退化为每 `10s` reminder。`SensorShort / SensorOpen / AdcReadFailed` 只停热并报告测温 fault，不蜂鸣、不进入 pending。owner-facing 温度显示在 RTD fault 期间保留最后一个有效读数，不再把 `0°C` 当作当前温度上报。
- `flux-purr thermal tune` 已接管 owner-facing `100w / pps5a` 5A full-batch orchestration；`flagship-tune` 只保留为兼容别名。默认同等级调优目标为 `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C`，按 `60, 240, 140, 100, 80, 120, 180, 160, 220` 的递归二分顺序执行。每个目标执行 target-local scout、offline retune、`current + 一个 evidence-specific predicted point` batch compare 与 promotion-gated `60s` hold confirm；已接受目标冻结，未调目标只以最近两侧 accepted point 的线性插值作为初值。真实 HIL 继续以 per-target budget 作为默认停止边界；dry-run 可用于 artifact contract 验证。
- 当 tuning scout 已经证明当前 profile 满足 warmup `100%`、dynamic full-speed gate 与确认裕量时，Rust orchestration 现在会直接把当前 profile 提升到 `60s` hold confirm，而不是再浪费一轮对当前点位的重复 batch retry；hold confirm 失败仍会保留证据并继续 target-local reseed。
- thermal self-test / flagship HIL 的 live host path 不再根据温度幅度、方向、斜率或残余热阶梯生成环境故障。`currentTempC`、`heaterFilteredTempC`、固件提供的 `heaterControlTempC` 与 `rtdRawAdcMv` 均保留在 raw samples 中，并由正式 overshoot / hold p2p / stable-window 指标评价。只有固件报告的传感器硬故障、过温、runtime/device、source telemetry 或持续采样率故障才能终止 live stage；`temperature_sample_glitch` 仅保留为历史 bundle 的解析兼容，新 Rust live path 不得由温度变化单独生成该原因。
- thermal self-test / flagship HIL 的 source telemetry stale recovery 现在先恢复 bench source，再重新下发 preview profile 并验证 `thermalProfileMode` / `thermalProfileResolvedBank` / `thermalControlProfilePreview` / `thermalControl.profileSource=preview`。若恢复后的目标已经掉回默认 `65w / pps3a` profile，CLI 必须立即把这次运行判为失败，而不是继续拿错误 profile 生成 tuning evidence。
- 历史 `scripts/thermal_tuning*` Python 工具链已经从正式 owner-facing / HIL 执行面移除。当前 supported orchestration/report path 固定为 Rust CLI：`flux-purr thermal tune` 与 `flux-purr thermal report rerender-legacy`。
- `flux-purr thermal report rerender-legacy` 现在作为正式兼容入口暴露：当现场已经留下旧的 `preliminary-review-*` legacy bundle，或已有一份 `thermal_self_test_preliminary_bundle` 需要在 Rust 路径下重写时，用它把输入目录重新写成 compliant preliminary review bundle，而不是继续依赖 Python rerender 脚本或把 legacy bundle 直接当作 owner-facing 最终报告。
- thermal tuning runner 在明确的 `SensorShort / SensorOpen / AdcReadFailed` 后只等待测温恢复并重试当前子步骤，不再把测温 fault 当作 attention reminder。若 runtime 报告真正的热失控 `faultAttentionPending`，runner 才发送 acknowledge；连续三次有效测试出现测温 fault 或热失控证据时，runner 会抛出 `thermal_alarm_pause` 并写出 `alarm-pause.json`，要求人工检查后重跑受影响测试。
- thermal tuning runner 对 `PUT /runtime` 采用异步写入语义：写入响应只作为请求确认，必须在同一 lease 下轮询 `/status`，直到 `targetTempC`、`heaterEnabled` 与关热时的 `activeCoolingEnabled` 都达到目标，或在有界超时后记录执行错误；不得把写入瞬间的旧 status 判为硬件失败。
- 当前 5A flagship bench fixture 固定为 Flux Purr 授权串口 `/dev/cu.usbmodem2111401` 与 IsolaPurr source `f293cc9c139e`（用户标识 `f293cc`）/ `http://192.168.31.224`。当前 repo-local sprint preflight 必须先确认 source readback 仍为 `100W`、PD enabled、PPS enabled、`pd_pps_5a=true`、`pps3_limit_ma=5000`、`tps_mode=auto_follow`。source recovery 只允许通过同一 source 的 runtime power gate 做掉电再上电：`isolapurr power runtime output --enabled false`、确认 `runtime.output_enabled=false` 且 USB-C 不再出力、等待 `2s`、`isolapurr power runtime output --enabled true`、确认 telemetry 推进并恢复 `auto_follow / 100W / PPS 5A`。授权串口缺失、变号或 source identity 变化时停止，不自动切换。
- `2026-07-27` 已在该 fixture 上完成 PPS 5A contract HIL receipt：repo-local `flux-purr -> devd -> espflash` 在授权串口烧录本 worktree 的 release artifact 后，设备 readback 为 `ppsCapabilityMinMv=5000`、`ppsCapabilityMaxMv=21000`、`ppsCapabilityMaxMa=5000`、`currentMa=5000`、`manualPpsEnabled=false`、`thermalProfileResolvedBank=pps5a`；source identity 为 `f293cc9c139e`，readback 保持 `100W / PD PPS / pd_pps_5a=true / pps3_limit_ma=5000 / auto_follow`。使用合规 5A eMarked 线材的短测以 `100°C` 运行 `120s`，约 `30s` 到达 `99.27°C`，随后采样保持在约 `99.3~99.9°C`；期间无传感器硬 fault、过温、runtime reset、source telemetry stale 或端口切换。结束后 `/status` 回读 `heaterEnabled=false`、`activeCoolingEnabled=true`、风扇处于冷却运行。该 receipt 只验证 APDO/5A 合同和短时热控路径，未调参、未保存 profile、未冻结或声明 `pps5a` accepted baseline。
- approach characterization 的 brake 搜索当前真相是：`timeout_without_valid_rollback` 在未进入目标带时、以及 `never_entered_approach`，都必须继续归类为 `more_heat`。否则高温点会错误回跳到更大的 brake，浪费真机轮次。
- `pps3a` 默认 seed 来自 committed 65W accepted bundle。`pps5a` 在 committed accepted bundle 缺失时回退到 repo-local tuning seed `thermal-self-test-runs/variants_100c_v6_hold220_cutoff90.json`。当前 100W 路径已具备 end-to-end bank、source metadata、preview/save/retune 语义，但 `pps5a` accepted EEPROM save 与 frozen baseline 仍未收口。
- thermal tune 默认同等级目标集为 `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C`；支持的完整 self-test ladder 另含 `200 / 250°C` 显式诊断目标。`300°C` 不属于首版验收。默认 host sampling interval 为 `300ms`，持续采样下限为 `3Hz`；accepted comparison bundle 可以使用更高采样率。
- 当前固件与 host 工具链已统一 warmup 语义：只要 heater state machine 仍在 `warmup`，输出就保持 `100%`，host readback / candidate import / replay / report 都必须把 `warmupPowerPermille=1000` 视为唯一有效运行值。
- heater 控制环当前为 `20Hz`；每个 control cycle 聚合 `64` 次 RTD ADC conversion，并丢弃前置 settle 样本后保留分数毫伏均值贯穿 calibration 与 PT1000 转换。默认 `tempFilterAlphaPermille=750`，仍可通过 thermal profile API / EEPROM 覆盖。冻结的 `pps3a` accepted bundle 仍记录 `700`，因此历史 3A bundle 与当前固件默认值应分开理解。
- 当前温度链路已经收口为三条可审计职责：owner-facing `currentTempC` / `boardTempCenti` / front panel 直接反映当前有效 RTD 样本；`heaterControlTempC` 反映实际送入控制器的最后可信样本；controller EMA 与 slope 继续单独暴露。PPS transition guard 与控制侧物理斜率门只能作用于 controller 内部状态，不得冻结或改写 owner-facing 温度。
- 控制环前不使用多样本窗口、中位数或输出钳位。控制侧仅以实际 `20Hz` 周期检查单样本是否超过 `35°C/s` 的物理斜率上限；被拒样本保留在 raw/report，并以 `heaterControlMeasurementGuarded=true` 审计，后续可信样本仍直接进入配置的 EMA。
- warmup handoff 现在要求实际温度步进确认，避免单个 RTD 批次跳变在滤波温度仍落后时提前退出 warmup。low-temp `Approach -> Hold` 零输出 seam 也已修正：只有当实际误差已进入 hold 释放带时才允许 predictive coast 维持零输出。
- host-side retune 已把 `warmupExitedAtMs -> firstHoldAtMs` 区间的 ideal Approach 曲线偏差纳入当前真相：当前 fit basis 固定为 `target_error_from_approach_start`，即用 `approachStartTempC -> targetTempC` 的归一化 target-error 曲线做 first-pass 分类，再决定是 `brake_late_or_residual`、`underpowered_or_early_coast` 还是 `oscillatory_near_target`。ambient 目前不是硬依赖；在样本未提供稳定 ambient 字段时，retune 不得阻塞，而是必须显式记录这一 fit basis。完成曲线分类后，retune 才继续区分 low-temp bounded residual 与 hold-entry carry。bounded residual 只做轻量 brake / cutoff / off 微调；只有明显 hold ripple 且 hold 输出高于基线时，才允许直接削减 hold sustain / reheat。
- `heaterCurrentReserveMa` 已进入 thermal profile settings、status 回显、preview/save API 与 EEPROM。heater safe-max 会在 source current capability 之上预留 reserve，而不是吃满整条 source 电流预算。
- `devd` 提供 localhost daemon、授权端口 serial discovery、lease、bounded events、USB identity/network/status/WiFi/runtime bridge、artifact verify、dry-run 与 real flash command boundary。真实烧录路径固定为 repo-local `flux-purr -> devd -> espflash`，并继续受授权端口纪律保护。烧录前 daemon 读取当前 partition table，对完整 `flux_cfg` record 进行备份写入与读回验证；app 写入后恢复该 record 至目标地址并再次验证，即使地址未变化。预写失败时拒绝 app 写入，恢复或验证失败明确报告为保护失败。
- `devd` 与相关 smoke 路径已固定几条 transport guardrail：显式 bind / serial / artifact root；授权串口缺失时拒绝自动切换到重新枚举端口；real flash 前释放 daemon-local serial session；浏览器与脚本通过 lease 复用同一设备会话而不是重复抢占串口。
- Web 将 native DEVD record 映射为顶部 target 时，主名称取固件 `identity.hostname`，缺失时取固件 `deviceId`；native USB product descriptor 不参与显示名回退。顶部 Radix Select 的 trigger 独立渲染当前 target 的主次信息，避免把下拉项的复合内容复制到固定高度触发器中。
- Web 设备选择器通过 `device-target-picker.ts` 以固件 `identityId/deviceId` 合并 native DEVD、browser Web Serial、direct WiFi/LAN 与 bridge records；每个物理设备只渲染一张卡片，卡片显示 hostname、设备 ID，并以固定顺序列出最多三种公开连接方式：`WiFi / LAN`（默认直连）、`Web Serial`、`桥接`。连接方式容器使用固定三列，因此一个可用方式保持三分之一宽度、三个方式恰好占满一行；顶部触发器清除通用 select 的 CSS 背景箭头，只保留组件内可旋转的 Lucide Chevron。DEVD USB 与 WiFi/LAN 只保留为桥接方式的内部来源，重复 transport 记录按状态优先级选择健康 target，连接方式按钮才会切换具体 target。
- Web Serial 连接从点击开始立即显示等待串口选择的反馈，并以 `15s` 有界事务结束；超时或取消后按钮恢复可重试，迟到的端口会被关闭。连接期间不会让此前的 devd `Failed to fetch` 继续冒充当前 Web Serial 结果；Storybook `Live / Web Serial connection timeout feedback` 覆盖等待、超时和恢复路径。
- ESP32-S3 executor 使用 `80 KiB` 的共享 task arena。HTTP 收发缓冲、解析请求、规范化控制命令和控制响应保存在 3 个静态 worker workspace，不进入 async task frame；每个 worker 使用独立 response signal，控制 mailbox 扩容以容纳并发读，但主控制循环仍是唯一消费者并串行执行 mutation。WiFi 初始化与 LAN task spawn 在 USB JSONL recovery 初始化之后执行，失败时发布网络错误但不阻断 USB 控制。
- `flux_cfg` 是当前 flash fallback 的正式分区。若升级前位于旧 factory-app 边界后的 raw fallback 双槽仍可读，runtime 读取它作为迁移源并立即复制到当前 `flux_cfg`；新记录只写当前分区，已被烧录覆盖的旧区域不视为可恢复来源。
- 当前控制平面已经具备 mock HTTP contract smoke、CLI-through-devd smoke、browser Web-to-devd smoke、runtime mutation/readback、artifact verify、flash dry-run、real flash、WiFi redaction 与 calibration/dashboard 关键路径的自动化或脚本化覆盖。

## Thermal acceptance state

- `pps3a` / 65W 稀疏 acceptance bundle 已存在，并继续作为 3A 当前基准。
- `pps5a` / 100W 路径已经具备 source-class 识别、bank 解析、preview seed、retune、approach characterization 与报告链路；`thermal-self-test-runs/approach-characterization-pd100w-pps5a-20260717-final/` 已作为当前 5A approach-reference 当前真相。
- `thermal-self-test-runs/preliminary-pd100w-pps5a-60-140-220-20260717/` 是历史 5A preliminary review bundle：顶层回显 `bundleDisposition=preliminary_review`、`acceptedProfileRole=review_candidate_snapshot`、`selectedMode=100w`、`resolvedBank=pps5a`、`detectedSourceClass=pps5a`，并为 `60 / 140 / 220°C` 三个 tab 分别附带 `holdCheck`。该历史三点 bundle 不满足当前 full-batch validation report 合同。
- 当前 100W 剩余工作是用 `flux-purr thermal tune` 收口 9 点同等级 full-batch preliminary review。历史 `0% / 25% / 50%` approach-only 曲线若要引用，只应作为显式诊断背景而不是当前默认判据。

## Remaining Gaps

- `pps5a` accepted EEPROM save 与 frozen baseline bundle 仍未完成；100W 路径虽已有 approach characterization current truth，但尚未形成 committed accepted bundle。
- WiFi-only pairing、DHCP/mDNS、lease competition、USB token reset，以及 Web LAN 的 Chromium PNA pairing/reload/control 闭环仍没有真机 HIL receipt；当前验证停在 ESP32-S3 cross-check、host tests 与 mock-only Chromium flow，不能宣称 LAN 控制已完成。
- Web direct LAN 的显式 CIDR scan 由浏览器直接并发请求受限私有 IPv4 范围的匿名 `/health`；该面板不经过 `devd`，并以独立单元测试和 Storybook play 锁定范围限制、取消、结果选择、CIDR 本地偏好恢复、扫描面始终可见及无 DEVD 依赖。恢复的 CIDR 不会自动启动扫描，首次面板也不伪造地址或网段。native `devd`/CLI 的 mDNS 与 CIDR discovery 保持为另一条入口。
- Add Device 的 Bridge 入口只展开 DEVD 二级选择面板，不再创建或选中 pending bridge device。面板把 USB 与 WiFi/LAN 候选分组，切换路径会清空具体设备选择，只有人工选中候选后才可提交连接；native serial records 显式标记为 USB bridge transport，browser direct LAN records 不参与该候选集合。
- Web 的 DEVD Bridge / WiFi-LAN 面板通过 `ControlPlaneHttpClient` 独立消费 `/api/v1/lan/devices`、显式 mDNS refresh 与显式 CIDR scan。进入该路径只加载 daemon registry，不扫描网络；两个 discovery 动作共享 loading 防重入并按稳定 LAN ID 合并候选，错误保留已有结果。CIDR 作为当前 origin 的非敏感偏好保存，hostname 是候选主名称，IP/base URL 与 paired 状态作为次要信息。Storybook play 使用注入式 client 验证 registry load、mDNS 和 CIDR 三条路径，不调用真实 DEVD 或设备。
- 固件 mDNS encoder 从 ESP task 适配层抽成纯 `no_std` 模块，host 测试锁定四条 DNS-SD answer、PTR shared class、unique record cache-flush、IPv4 与安全 TXT device metadata。DEVD Bridge 发现结果保持候选身份；确认按钮不会在配对、probe 与 LAN lease 尚未完成时写入顶部设备列表。
- `/dev/cu.usbmodem2111401` HIL 通过受保护烧录链验证 EEPROM WiFi SSID 与密码长度在重刷后保留。修复冷启动残留 apply signal 后，设备无需再次提交 WiFi 表单即可从 `connecting` 进入 `connected`，并在 `192.168.31.189` 返回匿名 `GET /health`，回显 device ID `a0f262f20d6c` 与 hostname `flux-purr-a0f262f20d6c`。Chromium direct LAN 已完成匿名 health、四位码 claim、顺序 bearer probe、30 秒 lease、hostname 自动注册与 WiFi target 自动选中；mDNS、SSE/runtime write/readback、lease competition/expiry 与 USB token reset 仍待真机 receipt，因此不能宣称 LAN HIL 已全部完成。
- 完整 artifact catalog 管理页不属于本 spec 范围。
- macOS 打开 ESP32-S3 USB Serial/JTAG port 仍可能触发一次设备 reset；`devd` 的稳定性契约是避免 Web / daemon polling 期间反复 open / close 造成持续重启。启动日志中的 USB reset marker 保持现有 fd 并等待 JSONL 启动响应，只有 recoverable I/O error 才允许重开。

## References

- `./SPEC.md`
- `../../solutions/device-control/thermal-control-self-test.md`
- `../../solutions/device-control/web-native-wifi-bridge-console.md`
- `../hhwq8-web-control-plane-demo/SPEC.md`
