# Flux Purr 热控调优

## Related ADRs

- [`0006-firmware-owned-thermal-tuning-core`](../../adr/0006-firmware-owned-thermal-tuning-core.md)

## 背景 / 问题陈述

现有 Web 控制台可以校准热模型和加热曲线，但不能执行正式的多温点 PID
闭环调优。现有 `flux-purr thermal tune` 由主机侧 CLI 编排，是重要的独立
参考实现；它不能成为 Web、直连串口或直连 LAN 的产品依赖，也不能在浏览器、
CLI 或 `devd` 断连时继续承担设备控制。

本规格定义一个由固件执行的正式调优流程。固件拥有实时调度、候选生成、评分、
硬门槛和安全收口；CLI 与 Web 只各自记录、回放、验证和呈现同一设备结果。旧的
主机侧算法长期保留为 CLI 可选的参考引擎，用于交叉验证及在修改固件前评估算法。

## 目标 / 非目标

### Goals

- 在控制台“校准”页新增“热控调优”子 tab，支持现有 DEVD Bridge、浏览器
  Web Serial 与直连 LAN 三种连接方式。
- 固件以一个可预测、定长内存、固定点 Rust 核心执行正式多温点调优；同一核心
  可在 native Rust 与 WebAssembly 中回放和验证决策。
- 固件将调优 trace 重传窗口、候选工作区和分页快照显式分配到板载 2 MiB PSRAM；
  内部 RAM 只保留实时控制热路径、指针与紧凑状态元数据，且不得在 PSRAM 分配失败时
  静默回落到内部 RAM。
- PSRAM trace ring 只保存尚未被主机确认的有界重传窗口，不保存整场报告。设备按全局
  `sequence` 分页传出事件；CLI 将 sample 原子追加到 `samples.ndjson`、将其余事件原子
  追加到 `decision-ledger.ndjson` 并完成 `sync_data`，Web 将页面与既有 sequence 合并后
  完成 IndexedDB read-write transaction。只有持久化成功的连续页面才可携带该页 rolling
  digest 发送 `ack_trace`，设备校验成功后才可回收对应 PSRAM 事件。
- CLI 与 Web 各自独立承担主机记录器职责，二者不通信；DEVD 只转发分页与 command，
  不缓存、补写或解释 trace。主机断连直至 ring 淘汰、主机持久化失败或任何 sequence
  缺口都会永久将该 run 标记为 `trace_gap/review_incomplete`。固件继续完成安全状态机，
  但不得 seal、preview 或 save 该 run 的候选。
- 只支持显式选择的 PPS `pps3a` 与 `pps5a` 调优方案；两种方案分别拥有候选
  profile bank，绝不 `auto` 解析、降级或换类重试。
- 在设备满足前置条件时，以九个温度点生成可审查的候选 profile，并经过设备侧
  硬门槛与完整主机 trace 审查后才允许 RAM preview 与 EEPROM save。
- 让浏览器与 CLI 分别拥有本地详细记录、报告导出和可选的参考比较，不建立
  Web-to-CLI 通信、转发或共同记录器。
- 保留现有主机编排算法作为显式 `host-reference` CLI 引擎；删除它必须获得主人
  的明确批准。

### Non-goals

- 不把 IsolaPurr、任何 bench source、VBUS 电流、电压或功率读取加入产品调优
  流程，也不在 Web 或 `devd` 中暴露电源准备/恢复操作。
- 不让 `devd` 拥有调优状态机、报告、记录器、CLI 子进程或 Web/CLI 数据中继。
- 不把既有 `thermalProfileMode=auto|65w|100w` 改成调优 API；它继续仅服务普通
  运行时兼容性。
- 不在旧固件或不支持 `thermal_tuning_run_v1` 的设备上回退执行主机参考算法。
- 不在调优中自动启动热模型校准、自动中止手动加热/其它校准任务，或在复位后
  自动续跑。

## 范围（Scope）

### In scope

- 新增 Rust `no_std` 调优核心、固件运行适配层、维护任务仲裁器与紧凑调优日志。
- 在 USB JSONL、设备 LAN HTTP 和 DEVD Bridge 中提供同构的
  `thermal_tuning_run` 协议。
- CLI 的固件引擎 runner、独立的 `host-reference` 引擎、参考比较和
  `thermal-tuning-v2` 报告。
- 编译给 Web 的 Wasm 回放/验证模块、浏览器持久记录器、ZIP 导出与控制台 UI。
- 所有 transport 的 mock/integration 覆盖，以及在获得单一精确端口授权后的 HIL
  验收。
- 与本主题相冲突的旧调优契约、HTTP 文档、CLI 文档和控制平面规格的迁移。

### Out of scope

- IsolaPurr 的操作页面、外部电源遥测、供电线缆自动选择和外部硬件接线诊断。
- 将完整样本或候选 profile 写入设备 EEPROM/flash；设备只保存紧凑、可恢复的
  最新 run journal。
- 在 Web 页面中嵌入或启动 CLI，或由 CLI 读取浏览器存储。
- 给非 PPS、自动选择的 profile mode，或第三种功率等级提供调优入口。

## 领域边界

### 调优核心与参考引擎

正式 **Thermal Tuning Core** 是无分配、固定容量的 Rust 状态机。它只接受规范化
的温度、VIN、PPS 合同、控制输出、时间和安全事件，输出下一步动作、候选、决策
账本事件和终态。固件是唯一推进 live state machine 的所有者。

核心的所有决策输入、候选字段、评分字段和 hash 使用固定点规范单位：时间使用
毫秒，温度使用 centiC，电压使用 mV，PPS 合同电流使用 mA，控制输出使用
permille。既有 heater PID 可以继续使用浮点实现；它必须把规范化观测传给核心，
而不能让浮点实现决定候选 hash 或门槛结果。

**Thermal Tuning Reference Engine** 是现有 CLI 主机编排算法的独立演进线。它可
复用 transport、数据结构、报告格式和比较规则，但不得复用正式核心的候选搜索或
评分实现。其存在用于发现固件算法差异、离线改进与受控 HIL 交叉验证，不能被
自动删除或改为普通 Web fallback。

### 运行归属

固件拥有 live run。CLI、浏览器或 `devd` 断连不取消 run；固件继续执行安全控制、
调度和终态收口。设备复位、掉电或启动失败则立即 disarm，写入
`interrupted_reset` 恢复摘要，不续跑，也不保留可 preview/save 的候选。
启动时已处于按下电平的前面板输入必须先被同步为既有状态；其释放不得合成任何用户
手势，更不得重新 arm heater。

CLI 启动的 run 由 CLI **Tuning Host Runner** 记录本机详细 trace 并生成报告；Web
启动的 run 由浏览器的持久记录器记录并生成同构报告。两者没有通信路径。`devd`
仅映射请求、复用既有 USB/LAN 连接和保存既有硬件登记信息。

### 功率等级

`pps3a` 表示 3A 级 PPS 策略，不是精确 `3000mA`：已有的 65W、`20V @ 3250mA`
合同属于该等级。`pps5a` 表示 5A 级 PPS 策略。固件根据其 PPS capability 与
选中等级的合同策略明确发布可用性。PPS APDO、线缆和合同电流必须被固件显式归类为
`pps3a` 或 `pps5a`；除了已知 3.25A/65W 属于 `pps3a` 外，产品不得依据“低于 5A”
等泛化规则自动接纳另一种等级。产品不得把一个请求默默改为另一个等级；能力不足
必须返回 `tuning_power_class_unavailable`。

## 需求（Requirements）

### MUST

- 设备必须在 capability 中发布 `thermal_tuning_run_v1` 与精确的
  `evidenceSchema=thermal_tuning_evidence_v2`，并列出可用的
  `pps3a` / `pps5a` 等级、固定目标集合、trace buffer 参数和候选 promotion
  支持。Web 只在该 capability 存在时激活新 tab 的操作。
- 每个调优开始请求必须显式携带 `powerClass: "pps3a" | "pps5a"`。不得接受
  `auto`、`65w`、`100w` 或未声明值。
- 固件必须在开始前检查：维护仲裁器空闲、设备 idle、活动热模型有效、加热曲线
  覆盖全部九个目标、所选 PPS 等级可用、温度测量和安全路径有效。它不得为满足
  条件而自动运行 `thermal_plant_auto`。
- 手动加热、自动校准、安装/恢复维护操作和 `thermal_tuning_run` 必须共用
  Maintenance Run Arbiter。冲突请求返回 `tuning_busy` 及当前 owner，不得隐式
  stop、cancel、resume 或抢占任何任务。
- 正式核心必须按固定顺序处理物理目标集合 `60 / 80 / 100 / 120 / 140 / 160 /
  180 / 220 / 240°C`，实际递归执行顺序为 `60, 240, 140, 100, 80, 120, 180,
  160, 220`。每个已接受 point 的最终参数冻结；失败 point 阻止所属区间继续
  细分，但不阻止已经满足边界的独立区间。
- 每个 target 的单调预算从 cooldown wait 开始，覆盖 scout、retune 和 confirm，
  上限为 20 分钟。核心不得有用户可调、会绕开此预算的无限 round 路径。
- 候选晋级必须满足 candidate-local 的实际非零 `warmup`、完整 stage、动态 full-speed-to-stable settle
  gate、`maxOvershootC <= 3.0`、`holdPeakToPeakC <= 3.0`，再完成 60 秒 hold
  confirm。动态 settle 的固定点上限为 `max(12_000ms, 2ms * max(0, targetCentiC -
  candidateStartTempCentiC))`，只从该 candidate 的 `scout` 起点计算；它必须小于
  60 秒 hold confirm，且绝不可复用上一 candidate 的温度或时间。所有门槛计算使用固定点；
  临界低裕量候选必须确认，不得直接 accepted。
- 核心必须以确定性的有界 perturbation ladder 生成 candidate。通过硬门槛的候选
  使用固定点字典序评分：最大正超调、hold 峰峰值、full-speed-to-stable settle
  时间、60 秒 hold 平均绝对误差、控制输出切换次数。并列时使用参数 canonical
  bytes 的字典序作为最终 tie-breaker。
- 固件必须仅将 device-local 温度、VIN、PPS 合同元数据、heater 控制输出和安全
  事件用作生产决策与证据。PPS 合同电流只是安全 ceiling，不得伪装为实测电流。
- 调优 trace 必须是全局单调 sequence 的有界分页事件流，至少包含
  `sample`、`phase_transition`、`candidate_trial`、`decision` 和 `safety` 五种事件。
  preview/discard/save 发生在 terminal trace seal 之后，其设备响应必须由 host recorder
  作为 post-seal promotion receipt 原子追加到 candidate 文件，不得并入已封存 rolling
  digest。host recorder 必须连续确认已持久化的 sequence 和
  rolling digest；若未确认事件将被覆盖、sequence 不连续或 digest 不符，设备必须标记
  `trace_gap` 与 `review_incomplete`，而不是静默丢样。
- 除明确排除的外部 VBUS/source 电压、电流、功率外，正式报告必须保留旧报告中所有
  可以从设备或 host recorder 真实获得的调优证据。每个 sample 必须携带目标、候选
  identity、trial 编号、阶段、时间、温度、VIN、PPS 合同、加热输出和测量有效性；每个
  candidate trial 必须携带完整固定点参数、起止 sequence/时间和样本范围；每个 decision
  必须携带完整 score vector、每个 gate、freeze、interval prune、disposition 和失败原因。
  每轮候选的 `candidate_trial` 起点只能在本轮 `cooldown_wait` 满足 `target-15°C` 预条件、
  即将进入 `scout` 后记录；此前的冷却 sample 仍是完整 target trace 的安全证据，但不属于
  该候选的评分样本范围，也不得计入它的 dynamic settle。
  每个候选都必须从自己的起点完成至少 5 秒 `scout` 预热，并在该候选的 `scout` 样本中观察到
  非零实际 heater output 才能满足 warmup gate；冷却样本或前一候选的输出绝不能满足这一 gate，
  也不能贡献下一候选的 overshoot 或 output-switch score。
  缺失字段必须以结构化 unavailable 标识呈现，禁止用相邻事件推断、显示空占位或静默
  删除来伪造完整性。
- 报告的目标卡、验收指标和候选详情必须引用同一个 candidate trial。存在 adopted trial
  时，候选详情默认显示该 trial，并明确显示其试验编号和 adopted 状态；其余 trial 必须
  保留为可切换的独立视图。目标卡必须显式标明采用 trial 的编号/总数，且其通过结论、
  overshoot 与峰峰值只能描述该 adopted trial，不能以“有效测试”或未限定指标暗示所有
  可见曲线均通过；终态 decision 的 `scoreSettleMs` 必须标作“目标评分 settle”，而
  candidate trial 自己的 `scoreSettleMs` 必须标作“候选试验 settle”，不得把这两个
  不同起点的时长当作同一指标。整个目标跨 trial 的 elapsed 可以单列为“目标总耗时”。
  每个目标的主温度响应图显示一个选中的候选 trial；试验切换控件
  必须提供试验编号、`rejected|passed|adopted` 状态、overshoot、峰峰值和
  gate 掩码。主图、控制图、设备电气图与详情默认显示 adopted trial，也必须随该控件同步
  切换到任一独立 trial；不得默认叠加全量 trial、不得把 rejected trial 的轨迹、峰值或任何
  指标视觉上归因于 adopted target card。每个独立 trial 从第一条非 `cooldown_wait` 事件
  开始，包含其实际存在的 `scout`、`retune` 和 `hold_confirm`；`cooldown_wait` 作为安全
  预条件必须保留在 trace 和候选审查中，但不得占用主响应图。每个试验时间轴必须按设备全局
  `elapsedMs` 严格递增，且不得拼接多个 trial 的本地时间轴。
- 报告绘图必须保持物理量纲。温度、加热输出、VIN/PPS 合同电压和 PPS 合同电流不得
  通过隐藏倍率共用同一数值轴；不同量纲使用独立图或明确标注的独立轴。`heaterOutputPermille`
  显示为 `0–100%`，其显式坐标范围不得扩展到负值。PPS 合同电流必须标注为合同安全
  ceiling，不能表示为外部 VBUS 的实测电流。
- 设备端 trace ring 只保存尚未由主机确认持久化的有界重传窗口，不是完整调优历史。
  当前正式固件必须在 PSRAM 中提供恰好 `1024` 条事件容量；按 500 ms sample 节奏，
  该窗口覆盖至少八分钟的连续 sample，并容纳其间的稀疏状态/决策事件。它不得占用内部
  RAM，也不得在 PSRAM 不可用时降级为更小容量或启动调优。所有 transport 每次 read
  最多返回 `8` 条事件，以保持 USB JSONL、LAN 和 DEVD 的同构响应上界。
- 主机遇到永久 `trace_gap` 时，设备仍必须返回成功的 snapshot，其中包含 active/terminal
  `runId`、`review.state=incomplete`、`review.reason=trace_gap` 与可读尾部 page。记录器必须
  先归档该尾部，不得发送 ack 或 seal；若 run 仍在运行，必须使用该 runId 发送 `cancel` 并继续
  归档至空 page，随后导出五文件的 incomplete bundle。不得因 trace gap 隐藏 runId、让加热任务
  因无法取消而继续运行，或伪造连续 trace。短暂 transport 失败必须有限重试，exactly repeated
  `ack_trace(throughSequence, traceDigest)` 必须幂等；相同 sequence 但不同 digest 必须失败。
- `review_complete` 只能由设备验证过连续 host archive acknowledgment 后产生。
  未 seal 的候选、trace gap、取消、硬故障、预算耗尽、reset 中断或失败 run 均不得
  preview 或 save。
- preview 必须用 `runId + candidateId + candidateHash + powerClass` 精确绑定，
  只把 candidate 写入 RAM active bank、回读验证且不启动加热。save 必须在一次
  preview 成功后，由第二次简单确认触发，将同一未改变 candidate 写入相同 EEPROM
  bank；不得自动 save。
- 设备 journal 每个正常 run 最多执行两次逻辑 journal 写入：开始 marker 与 terminal
  summary。发现未闭合开始 marker 的启动恢复只补一条 `interrupted_reset` terminal
  summary。raw trace 和 promotable candidate 不得持久化；profile save 是独立的用户
  配置写入，不计入 journal 预算。
- Web 和 CLI 必须导出同样的 `thermal-tuning-v2` 文件集；Web 以浏览器本地生成 ZIP
  导出。新产品工作流不得查询外部设备的 VBUS telemetry，也不得要求文件上传、
  密码、口令或审批 token。
- CLI 必须同时保留 `--engine firmware` 产品 runner 与 `--engine host-reference`
  独立算法。reference comparison 的结果只能是 `equivalent`、`divergent`、
  `inconclusive` 或 `not_run`，且不能阻止设备候选 preview/save。

### SHOULD

- 设备状态应在 trace 溢出前暴露确认滞后和可用 buffer，允许 Web/CLI 提前提示记录
  器风险，而不暗示设备已经停止。
- 设备应暴露紧凑的 last-run journal summary，使重新连接的操作者能区分
  `interrupted_reset`、正常终态与无活动 run。
- Web 应自动以 `deviceId + runId` 为键写入浏览器持久存储；存储配额或事务失败应
  立即显示记录失败，并保持设备 run 的真实状态。

## 功能与行为规格

### 启动与预检

1. Web 显示“热控调优”tab；CLI 的固件 runner 读取相同 capability。缺少
   `thermal_tuning_run_v1` 时，界面明确说明固件不兼容，且不显示 host-reference
   fallback。
2. 操作者选择 `PPS 3A` 或 `PPS 5A`。界面展示固定九个目标、当前 capability、
   所选等级、模型/曲线/PPS/idle 预检结果和维护冲突 owner。
3. 点击开始后只显示一次简单确认对话框；不要求输入文本、密码、口令或 token。
   确认后才发送 `start`。没有满足条件时开始按钮不可用，并保留设备返回的具体
   `tuning_ineligible` reason。
4. firmware 通过仲裁、预检后创建 run ID、写 start marker、选择对应 PPS bank 和
   合同策略，并开始输出 sequence 事件。`devd`、LAN 与 USB 只转发这一结果。

### 固件调优状态机

每个 target 的每一个候选 trial 都独立经过 `cooldown_wait`、`scout`、`retune` 和
`hold_confirm`，然后才可参加 `accepted|failed|skipped` 的 target 决策。完成一个候选后，
下一个候选必须重新回到 `cooldown_wait`，直到温度不高于 `target-15°C` 才开始它自己的
`scout`、warmup、approach 与 dynamic settle；不得从上一候选的 hold 或 retune 直接继续。
`scout` 从该候选的 `candidate_trial` 起点单独计时，至少持续 5 秒；它不得复用 target 或前一
候选已经消耗的 scout 时间。
核心在进入 target 时只从最近两侧 accepted boundary 做线性插值得到 seed；没有两侧边界时
使用该等级的已持久 baseline。它不能把某个 point 的最终参数回写到另一个 point。

任何 PPS 合同丢失、温度测量失效、固件 heater safety、运行时硬故障、操作取消或
target budget 耗尽都按明确 terminal/target disposition 收口。取消与硬故障立即
disarm；普通失败 point 的区间裁剪不影响其它合法子区间。核心记录每个状态转移、
候选、score、gate、freeze、interval prune 和终态决策，供 host 归档。

### Trace 记录、确认与断连

每个 `sample` 或 `decision` 事件有唯一递增 `sequence`，并按 canonical bytes
并入 rolling digest。设备在 PSRAM 维护恰好 1024 条未确认 ring buffer、最早可读
sequence、最近确认 sequence 和当前 digest；一次 page 最多 8 条事件。host 读取 page 后
持久化事件，再发送连续 `ack_trace`；设备只接受从上一次确认连续前进且 digest 匹配的 ack。
主机因响应丢失重发同一 `(throughSequence, traceDigest)` 时，该 ack 必须幂等；同一 sequence
携带不同 digest 必须失败。

CLI、浏览器或 `devd` 断连不会影响调优控制。只要 host 在 buffer 覆盖前重新读取
并确认全部事件，run 仍可得到 review-complete；否则设备永久标记
`trace_gap/review_incomplete`。发生 gap 的 snapshot 仍携带 runId 与可读的 ring tail，
记录器归档尾部但不确认，并在 run 尚运行时发送 cancel，直到得到 terminal/空页；由此产生的
五文件 bundle 必须明确为 incomplete。host 完整确认到 terminal sequence 后发送
`seal_review`，设备以其紧凑 digest 和 sequence 验证完整性，并仅在当前 boot 的
RAM 中标记 candidate 可 promotion。浏览器页面关闭、CLI 退出或浏览器 storage
失败不会使设备不安全，但可能使 candidate 不可 promotion。

### 候选 preview、save 与恢复

成功且 review-complete 的 run 产生确定性的 candidate ID 和 hash。Web/CLI 可以
对候选执行 preview；设备验证 run、ID、hash、power class 和 review 状态后，将它
应用到 RAM bank、回读并返回 applied hash。preview 不使 heater enabled，也不写
EEPROM。操作者可以 discard preview，设备恢复该 bank 的启动前 RAM profile；设备
重启同样从已持久 profile 恢复。

save 操作必须使用与 preview 相同的四元组，且 active RAM profile 的 hash 必须等于
candidate hash。UI/CLI 在 save 前展示第二次简单确认。成功保存后设备写入精确的
`pps3a` 或 `pps5a` EEPROM bank，返回持久化 revision 和 hash；不会改变普通运行时
`thermalProfileMode` 的兼容语义。

### CLI、Web、Wasm 与报告

`--engine firmware` 的 CLI runner 与 Web 共享 device 协议，但各自创建本地 archive。
Web 使用 Wasm 复算 candidate/hash/decision ledger，CLI 使用同一 Rust crate 的
native build；两者只能验证或展示设备 live 结果，不能推进 live state。

CLI 的 `host-reference` 继续保留自己的 optimizer。它可以将相同输入投影为 reference
ledger 并与 firmware bundle 比较；这是诊断/HIL/release 证据，不能改变 runtime
candidate 的 eligible 状态。历史 reference bench diagnostics 可以保留在该明确
选择的开发流程中，但 `--engine firmware`、Web 和 `thermal-tuning-v2` 不得依赖
外部 VBUS telemetry。

报告格式、CLI 入口和控制平面操作见：

- [control-plane.md](./contracts/control-plane.md)
- [cli.md](./contracts/cli.md)
- [file-formats.md](./contracts/file-formats.md)

### Web UI

校准页的新子 tab 是一个操作面，而不是说明页。它包含：capability/预检状态、PPS
等级 segmented control、开始/取消确认、当前 target 与阶段、设备状态、trace recorder
健康度、九点进度、候选结果、preview/save/discard 操作和报告导出。它不包含 source
控制、VBUS 仪表或外部设备操作。

现有热模型校准和 heater-curve tab 保持独立。设备处于其它维护任务时，调优 tab
显示占用者并禁用开始；不会提供“替换当前任务”的按钮。三种 transport 通过现有
`ControlPlaneTransport` 抽象实现相同状态和错误文案，直接串口只依赖已授权的浏览器
端口，LAN/Bridge 继续使用既有 lease/pairing 权限，不新增认证步骤。

## 接口契约（Interfaces & Contracts）

| Surface | Contract | Responsibility |
| --- | --- | --- |
| Device protocol | [control-plane.md](./contracts/control-plane.md) | live run、trace、ack/seal、candidate promotion |
| CLI | [cli.md](./contracts/cli.md) | firmware host runner、reference engine、comparison |
| Report/export | [file-formats.md](./contracts/file-formats.md) | portable archive、Web ZIP、legacy import boundary |
| Shared Rust | this specification | `no_std` core; native/Wasm deterministic replay API |

## 验收标准（Acceptance Criteria）

- Given 支持 `thermal_tuning_run_v1` 的设备，When Web 通过 DEVD Bridge、Web Serial
  或 direct LAN 连接，Then 校准页均出现相同的“热控调优”tab、`pps3a`/`pps5a`
  选择和设备驱动状态；不会调用 CLI 或外部电源 API。
- Given 缺少 capability 的旧固件，When 打开该 tab，Then 操作明确为不兼容且没有
  host-reference、自动 mode 或另一功率等级的 fallback。
- Given 已选 `pps3a`，When 设备的 3A-class PPS 能力可用（包括 65W/3250mA），Then
  firmware 启动该 bank；Given 选错或不可用 class，Then 返回
  `tuning_power_class_unavailable` 且不改用 `pps5a`。
- Given 任何模型、曲线、PPS、idle 或安全预检不成立，When 请求 start，Then 设备不
  写 start marker、不加热，且返回结构化 `tuning_ineligible` reasons；不会启动热模型
  自动校准。
- Given 手动加热、自动校准或另一维护任务活动，When 请求 start，Then 返回
  `tuning_busy` 与 owner，且现有任务不受影响。
- Given 合法 run，When 固件处理九个目标，Then decision ledger 的目标顺序、区间
  依赖、冻结/裁剪规则、候选 hash 和固定点 golden replay 在 firmware、native 与 Wasm
  完全一致。
- Given host 断连，When 设备仍有供电且安全条件正常，Then run 继续或按自身安全规则
  收口；Given buffer 覆盖或 sequence/digest 缺口，Then terminal result 为
  `review_incomplete`/`trace_gap`，candidate 不可 promotion。
- Given 设备 reset 或掉电，When 下次启动，Then heater disarmed、journal 显示
  `interrupted_reset`、run 不续跑、旧 candidate 不可 preview/save。
- Given review-complete candidate，When preview 后再以相同 run/ID/hash/class save，Then
  preview 只写 RAM、不启动加热，save 经第二次确认后只写对应 EEPROM bank；任何
  mismatch、未 preview 或 reset 后请求都失败。
- Given CLI 或 Web 完整记录一个 run，When 导出，Then 生成同样的 `thermal-tuning-v2`
  文件集合，sample 与 ledger 无缺口，且脱机 `index.html` 能显示主结果与逐点详情。
- Given firmware bundle 与 host-reference comparison，When 结果为 `divergent`、
  `inconclusive` 或 `not_run`，Then 它被记录为诊断结果，且不阻止设备已
  review-complete candidate 的 preview/save。

## 实现前置条件（Definition of Ready）

- 已冻结本规格、ADR 和三个接口契约。
- `thermal_tuning_run_v1` 所需的 firmware memory budget、PSRAM trace ring 容量、
  控制平面间接快照尺寸与 EEPROM journal layout 已通过编译期/单元测试预算检查。
- Web Wasm toolchain 能在现有 Vite/Bun 构建中生成受版本锁定的模块。
- HIL 阶段取得单一精确 MCU 端口和相应主人授权；在此之前只执行 mock、仿真与
  非写入验证。

## 非功能性验收 / 质量门槛

- `thermal-tuning-v2` 的 report bundle 必须能从事件 union 重建九个目标、每个候选
  trial、每个阶段转换、每个 decision、每个安全收口，并从 post-seal receipts 重建每个
  preview/discard/save 操作；报告
  renderer 必须逐字段校验这一覆盖，任何缺失都只能输出 `review_incomplete`，不能输出
  completed/candidate-ready 的完整报告。
- 核心在 `no_std`、native 与 Wasm 三个 target 上运行相同 golden vector，禁止使用
  target-dependent floating-point 作为决策输入。
- firmware、USB JSONL、LAN HTTP、DEVD adapter、CLI、Web 和 report parser 都必须
  具备 schema/negative-path 测试；直接串口、direct LAN 和 bridge 的行为保持同构。
- `--engine firmware` 与 Web 测试必须断言没有 IsolaPurr、VBUS meter 或其它外部
  telemetry client 被创建。reference-engine 专用 fixture 与产品 fixture 分开。
- Web 改动需要 Storybook/组件交互覆盖、浏览器可控 visual evidence 和桌面/移动
  截图审查；真实设备截图仍须逐次取得主人授权。
- HIL 必须覆盖两个 PPS class、断连、trace overflow、reset recovery、candidate
  preview/save 和 reference comparison；HIL 不得以 `mcu-agentd` 替代现有
  `flux-purr` through `devd` 验收路径。

## 文档更新

- 实施时同步更新 `docs/interfaces/http-api.md`、CLI help/reference、控制平面协议
  文档和 `docs/specs/m8r4q-real-control-plane-runtime/SPEC.md`。
- 将 `q2aw6-heater-pid-frontpanel-runtime` 与任何把主机 CLI 说成正式调优 authority
  的已完成规格改为兼容历史，并引用本主题与 ADR。
- 保留 `thermal-profile.accepted.json` 的 import-only 说明；不得把它重新列为新产品
  导出物。

## 实现里程碑（Milestones）

- [x] 建立 `no_std` thermal-tuning core、canonical codec、hash、golden vector 和
  native/Wasm bindings。
- [x] 将 core 嵌入 firmware，完成仲裁、PPS class 预检、状态机、trace ring、ack/seal、
  journal 和 candidate promotion。
- [x] 在 USB JSONL、LAN HTTP 和 DEVD Bridge 实现同构协议与 capability 映射。
- [x] 增加 CLI firmware runner、`thermal-tuning-v2` writer、reference-engine 保留层和
  nonblocking comparison。
- [x] 增加 Web Wasm replay、浏览器持久记录器、ZIP writer 和所有 transport client
  方法。
- [x] 在校准页实现热控调优子 tab、确认流程、candidate 操作、Storybook 与可控视觉
  证据。
- [x] 收敛旧规格/接口、完成 mock/integration compatibility suite。
- [ ] 在获得明确硬件授权后完成两种 PPS class 的受控 HIL receipt。

## 风险与开放问题

没有待主人决定的产品边界。实施前必须量化并验证以下工程风险：固件 PSRAM 中 trace
ring 的可用容量、内部 RAM 热路径余量、EEPROM journal 的原子恢复行为、Wasm/native canonical hash 一致性、
浏览器持久存储配额，以及两个 PPS class 上固定点候选梯度的物理效果。任何需要扩大
功率等级、恢复外部 source telemetry、删除 host-reference 或增加 Web/CLI 通信的
变更都需要新的主人确认和 ADR 审查。

## 假设

- 设备现有 PD 控制器能发布并维持 `pps3a` 与 `pps5a` class 所需的 PPS 合同；不能
  满足的设备通过 capability/eligibility 显式拒绝。
- 普通运行时 profile bank 与新的 candidate bank 可以按 `pps3a`/`pps5a` 一一映射，
  而不改变 `auto|65w|100w` 的既有解析语义。
- CLI 与浏览器均可在各自本机持久化详细记录；设备只承担紧凑摘要和位于 PSRAM 的
  有界未确认 trace 缓冲。

## Visual Evidence

实现包含受控的 mock-only Storybook 视觉证据，覆盖桌面工作面和 `393x852` 移动视口。
截图只展示 fixture 数据，不连接真实设备，也不包含硬件、外部电源或敏感信息。

- [`thermal-tuning-ready-desktop.png`](assets/thermal-tuning-ready-desktop.png)：
  `storybook_canvas`，`Calibration/ThermalTuningRunCard--Ready`，桌面默认视口；
  `state=ready-pps3a-nine-targets`，`capture_scope=component`，
  `target_program=mock-only`。
- [`thermal-tuning-ready-mobile.png`](assets/thermal-tuning-ready-mobile.png)：
  `storybook_canvas`，`Calibration/ThermalTuningRunCard--ReadyMobile`，
  `requested_viewport=393x852`；`state=ready-pps3a-nine-targets-responsive`，
  `capture_scope=component`，`target_program=mock-only`。

两张图均保留组件外边距，用于审查焦点、标签、状态提示和窄视口下的布局边界；
它们是组件级 owner-facing 证据，不代表真实硬件运行结果。
