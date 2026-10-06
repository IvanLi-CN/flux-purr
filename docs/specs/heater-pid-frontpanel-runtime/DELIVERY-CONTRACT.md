# 热控调优交付合同

## 目的

本合同收敛 Web App 加热参数调优任务的原始目标、后续边界和验收方式，避免把热控调优、PD 协议恢复、显示启动和无关分支问题继续混在同一交付中。

它是本主题的工作入口，而不是新的协议或安全规范。出现冲突时，以下现有规范优先：

- `docs/specs/heater-pid-frontpanel-runtime/SPEC.md`：热控状态机、保护、PID 行为和热端验收；
- `docs/specs/real-control-plane-runtime/SPEC.md`：Web、CLI、devd 与固件控制面契约；
- `docs/specs/fusb302b-dual-pd-sink/SPEC.md`：PD 协议、恢复和状态投影；
- `docs/specs/eeprom-memory-config/SPEC.md`：profile、thermal plant 与持久化原子性。

实施开始前必须重新确认工作树与 `origin/main` 同步。本文建立时的已验证基线为 `d86a4eddccfc9d31cbe9b9cb2f2d597b8b525278`，其中已包含 PD 模块化与恢复改动。

固件主控调优按[整改合同](FIRMWARE-TUNING-REMEDIATION-CONTRACT.md)执行。Approach 使用制动优先的模型控制。Warmup 保持独立全功率和100ms物理软启动；Warmup→Approach 的首个正常控制周期立即请求 `50‰`（5%），让已经输入的余热继续传递。随后每个周期以实际/滤波误差、滤波升温率、标定自然损失、热容量及传热延迟决定补热，正常请求始终为 `50..1000‰`，不把进入 Approach 后的输出强制为单调下降，也不固定为50%默认曲线。预测余热必须允许抵扣散热补偿，不能用目标温度的完整损失形成隐含功率下限。过冲截止、传感器/电源故障等真实保护可以停热；普通 predictive coast 不能把 Approach 请求置零。brake/lead/damping 调整制动位置与响应；保留的 profile floor/ceiling 不钳制有效模型轨迹。固定物理 Hold 入口、稳定窗口和独立60秒确认保持不变。 模型制动控制使用 timing policy `4`，绑定 `modelApproachFloorPermille=50`、`allowedApproachFloorPermille=50` 和生产有效下限 `50‰`。候选配置值仍如实记录用于身份核验，不能抬高实际模型下限。原期限首次检查时仍须处于Approach、尚无期限内有效稳定窗口、期限前取得真实50‰请求且之后未提高非零请求，才可按原有限上限延长。期限、稳定窗口、热指标和独立确认均不放宽。历史policy `3` 保留100‰模型规则，policy `2` 保留100‰配置/Hold派生规则，不能以新政策重写旧报告。 固件高温高侧保守减能例外保留，外部方案冻结；模型输出不得由保留的profile floor/ceiling或Hold基线钳制。

## 问题陈述

项目需要让操作者能够从 Web 控制台安全地完成热控参数的查看、预览、保存、调优和结果复盘。此前实现和验证反复被以下因素打断：

- 调优候选虽被选中，却没有明确证明真实热控循环使用该候选参数；旧热模型控制器还可能限制热端输出；
- PD 处理曾与 50 ms 加热循环耦合，无法满足协议软复位等短时限；
- PD、显示黑屏、真实烧录和脏工作树同时进入同一任务，导致每次结果的基线和证据均不稳定；
- 构建或 mock 成功曾被误当成真实热端闭环已经通过。

## 原始需求

1. 确认并补齐 Web App 的加热参数调优能力，而非只展示只读温度。
2. 支持查看当前热控 profile、在客户端编辑参数、预览候选、保存通过校验的 profile，并在状态页读回有效来源和参数。
3. 支持受保护的标定/调优操作，展示执行状态、遥测、失败原因和可复盘结果。
4. 候选 PID 或 profile 必须实际进入相应的调优热控路径；调优输出不得受候选 profile 外的隐式控制器或参数限制。
5. 任何 Web 操作都不得绕过 PD、传感器、EEPROM、风扇、过温和故障锁存保护。

## 后续补充需求

### PD 与加热的职责边界

- PD 是独立的协议服务，按 FUSB302B 规范以 5 ms cadence 运行并独占短时 I2C 访问；加热控制是 50 ms 闭环，只消费经过验证的电源状态投影。
- 加热允许条件只能来自 `PowerState` 和既有安全状态；不得在 Web、devd 或加热循环中重写 PD 协议状态机、直接写 PD PHY，或根据历史合同推断当前合同仍然有效。
- PD 逻辑已作为主干基线完成。本交付只验证其对热控的状态投影和 fail-safe gate，不修改 `fusb302b`、`pd_service` 或 `power_domain` 的协议与恢复逻辑，除非另行建立独立需求和验收。

### 本文术语：旧方案与生产控制兼容锚点

“旧方案”只在本合同中指 `origin/main@d86a4eddccfc9d31cbe9b9cb2f2d597b8b525278` 的外部主控 `flux-purr thermal tune` 5A full-batch 调优：主机经 devd 和外部 bench source 安排 scout、离线 retune、candidate batch、60 秒 hold confirm 和四文件 preliminary review bundle。无 `--engine` 的 CLI 默认行为、外部 source 参数和 source telemetry 仍属于该方案。脱离本合同，不得单独用“旧方案”作为实现或验收基线，须写全“外部主控调优方案”并指向该主干基线。

`ff4bf3876f548bc4ba6ac1e2cb87259f0d132422` 仅是独立的生产热控闭环兼容性锚点，不是“旧方案”的调优策略或报告基线。其 3A plant PI、物理模型门禁和共用安全/执行器边界仍作为兼容依据；5A 控制器选择遵循 [source-class thermal controller decision](../../adr/0015-source-class-thermal-controller-selection.md)，不要求与该锚点的 5A plant PI 输出逐值相同。

共用生产边界包含：物理 thermal plant 模型门禁、50 ms 单调控制节拍、PPS 电压/PWM 映射、100 ms warmup 软启动、传感器/过温/供电保护，以及独立的 `thermal_plant_auto` 受保护瞬态和原子激活。3A plant PI 保留其热平衡、预测、饱和和 anti-windup；5A point-local 控制器保留其 warmup/approach/hold、有效参数和 PI 语义。

生产控制兼容行为如下，调优实现必须逐项保留：

| 部分 | 固定行为 |
| --- | --- |
| 模型与门禁 | 两类生产加热都仅在有效 physical thermal plant model 存在时允许。3A 继续使用 `Ctheta*dT/dt = Ph - kc*(T-Ta) - kr*(Tk^4-TaK^4)` 和 `sat(Ploss(T) + Kp*(e-tau_d*dT/dt) + Ki*integral(e))` 的 plant PI，保留 `Pmax(T)` 饱和与 anti-windup。已核实的 5A PPS source 使用现有 point-local profile 控制器；其输出仍受既有 PPS/PWM、温度相关电阻和电流安全边界限制。两者控制目标均为请求温度，不得使用隐藏 target offset。`Pmax(T)` 是建模上限而非实测功率。 |
| 节拍与采样 | 控制循环固定为真实单调 `50 ms (20 Hz)`；每周期聚合 `64` 次 RTD conversion，采样总频率至少 `1280 Hz`。PWM 固定为 `100 Hz`，并回显控制间隔、周期耗时、滤波升温率和 coast 状态。 |
| Warmup | 首次 arm、目标切换或 `Approach -> Warmup` 回退均执行 `100 ms` 线性软启动。warmup 内逻辑输出固定 `100%`，沿温度/电流合同允许的 PPS 上限运行；不得用 profile 中的较低功率把它变成慢爬坡。 |
| Approach 与 Hold | 普通5A与内部调优共用自然冷却模型辅助的生产 profile 路径，制动、预测及补热恢复按 [自然冷却控制合同](NATURAL-COOLING-CONTROL.md) 执行。首次进入 Hold 后仍锁定 PPS 电压，由 PWM 快速 PI 调节；Hold 不主动下扫 PPS。物理入口与资格门禁保持固定。 |
| 执行器映射 | PPS 提供粗粒度功率控制；PPS 高于工作 floor 时保持 `100%` PWM，达到 floor 或 bounded down-ramp 时，PWM 按请求功率与当前电压等效功率连续补偿至 `0..100%`。持续满 PWM 且升温不足时，才允许至少间隔 `2 s`、每次最多 `500 mV` 上调 PPS。 |
| 参数与 profile | 运行时按目标温区选择完整 point-local 参数：brake、warmup reentry、approach、hold、PI、blend、lead 与各阈值。`settings` 只承载规定的全局字段。任何 preview/save 必须把当前目标的 effective point 和 settings 完整、可读回地 materialize。 |
| 物理模型标定 | `thermal_plant_auto` 是独立单次事务：选择覆盖 `20V / >=3A` 的一个 PPS APDO，使用该 APDO `max_mv` 与 `100%` PWM 加热到 `220C`，立即断开 MOS，在保持合同下被动冷却至 `80C`，由同一 trace 拟合并原子激活。它不进入 Approach/Hold，也不生成 PID candidate。 |
| 安全边界 | PPS/AVS capability、current reserve、传感器故障、EEPROM/thermal-model lock、绝对过温、合同丢失和 terminal disarm 始终优先于控制输出；任何安全拒绝都保持 heater 停用并留下可观察原因。 |

生产闭环没有一套独立的“自动调参控制器”。新 `thermal tune --engine firmware` 只能在闭环之外基于运行证据搜索 point-local 参数、写入 RAM preview、让正常 5A 生产闭环执行试验，并在通过验收后保存；它不能接管或复制生产控制。

1. 调优 orchestration 只能生成候选、安排试验、收集证据并请求 preview/save；每一轮真实加热必须调用生产兼容锚点的同一条控制路径。不得新增第二个 heater state machine、第二套功率公式、隐藏 PID 常量、专供调优的输出限幅，或用 host 推断温度来取代固件控制决策。
2. 5A 正常加热与 5A 调优必须共同选择既有 profile 控制器，使 effective parameters 真正进入同一生产路径；这是相对于生产锚点的明确算法选择差异。3A plant PI、PD 独立 5 ms 服务、PPS/PWM 映射、调度、保护和 `thermal_plant_auto` 不得被此选择修改。切换电源类别前必须停热并重置控制器状态，不得在一次 armed 运行中热切换。
3. 本交付的固件调优只解析到 `pps5a` bank；`pps3a` 请求不得加热、生成候选或保存，只能生成失败审计。既有 profile preview/save 操作仍按 selected mode 使用 `pps3a` 或 `pps5a`，不得因本交付被削弱。固件调优的 5A 有效 point 和 settings 必须完整 materialize 并由 status 逐字段读回；不得借用另一个 bank、旧 preview 或隐藏残留。
4. 除上述已批准的 5A 控制器选择外，任何需要修改生产算法、PD/PPS 语义或 3A 路径的请求仍须先更新长期 SPEC、列出与 `ff4bf387…` 的精确差异，并取得主人明确确认。

### 两种调优 engine 的固定差异

| 边界 | 外部主控调优方案（本合同内的“旧方案”） | 新固件主控 5A 调优 |
| --- | --- | --- |
| 选择与范围 | 无 `--engine` 时维持原 CLI；外部 runner 主控 5A full-batch | 仅显式 `--engine firmware`；资格范围为完整 `pps5a` 九目标，`pps3a` 不加热并失败 |
| 策略与生产控制 | 主机执行当前主干的冷却、seed/插值、scout、batch compare、score、gate、预算和 hold confirm；设备生产闭环负责加热 | 固件策略对冻结的 5A 可观测向量保持决策一致，60°C 起点同为 `<=35°C`；全程全速冷却及终止后释放按 [60°C 冷却起点合同](FIRMWARE-60C-COOLDOWN-CONTRACT.md) 执行；设备仍使用同一生产闭环，不能调用或复制外部 runner |
| 数据来源 | 外部 source client 的 VBUS/source telemetry 与设备状态均可进入原报告 | 仅设备本地温度、VIN、PPS 合同/请求和控制证据；外部 VBUS、物理电流/功率不得推断或补造，PPS 合同电流仅为安全 ceiling |
| 命令输入 | `--source-id`、`--source-url` 等外部 source 参数保持有效 | 拒绝外部 source 参数；设备 tuning command/status/event stream 按 runId 和 sequence 读回 |
| 报告 | 原四文件 preliminary bundle 和 `thermal-profile.accepted.json` 保持原义 | `thermal-tuning-v2` 五文件、完整 sample/ledger、trace/digest、可切换 trial 与二元资格结论 |
| 候选与持久化 | 原审查候选快照不等于已保存 bank | candidate、RAM preview receipt、seal、二次 confirm-save、EEPROM bank generation/hash/readback receipt 严格分离 |

无法由设备本地事实映射的外部 source 判定不得伪造为等价向量；应保留 unavailable 与失败原因。若保持已冻结 5A 决策需要改变策略、生产控制或 PD/PPS 语义，必须先给出 A（现有本地事实适配）与 B（改变合同）的差异、风险和验收影响，停止在当前 PR 静默修改。`pps3a` 调优本身只可在明确新请求后进入独立任务和 PR。

### 热控的两类操作

- `thermal_plant_auto` 是受保护的一次物理 thermal plant 标定：它执行既有受控瞬态、在同一运行内原子写入 heater curve 与模型，不暴露候选审批或 preview-to-save 流程。
- PID/profile 调优使用既有 thermal profile preview/save 契约：候选仅在 RAM preview 生效，必须从设备状态读回 resolved bank、source、target coverage 与 effective parameters；只有定义的调优运行整体通过后才可保存。
- 两条操作都必须清楚标示目的、当前状态、可取消性、失败原因和持久化结果，不能共用一个含糊的“校准”语义。

### 交付纪律

- 实现和证据只能建立在干净、与 `origin/main` 同步的工作树上；不得从无关分支或未验证的本地提交继承结论。
- 不把显示初始化、背光或前面板渲染改动捎带进来。只有与本流程直接相关的回归才可修复，并须单独说明。
- 构建、mock 和离线报告是软件证据，不等于硬件热端验收。真实 HIL、串口读写、复位和烧录必须另获主人对精确端口及操作的授权。

## 范围与非目标

### 范围内

- 真实控制台的 thermal profile、heater curve 和 calibration 工作流及其 typed control-plane adapter；
- Web 表单校验、preview/save/clear 的状态读回、作业状态、错误呈现和报告入口；
- 固件、devd、CLI 和 Web 之间的 profile 与 calibration 契约一致性；
- 热控调优候选确实驱动预期控制路径的自动化验证；
- 本主题的文档、测试和可视化证据。

### 范围外

- FUSB302B 的协议、CC、PHY、重置和合同恢复实现；
- 无授权的真机操作、烧录、串口监视或端口自动切换；
- 把 Web 控制台变成直接绕过 devd/既有 transport guard 的硬件写入通道；
- 无关的显示、网络、外观或分支清理工作。

## 实施方案

### 1. 固定基线和职责

1. 从同步的 `main` 创建干净工作空间，记录实现前后的 `HEAD...origin/main` 差异和本任务文件列表。
2. 将 PD 视为输入：只经 runtime status 中的 `PowerState`、合同有效性和安全锁读取其结果；增加测试保护，避免热控改动触碰 PD 协议模块。
3. 以现有 topic SPEC、real-control-plane 规范和 EEPROM 规范为唯一行为来源；本合同只协调工作，不复制或降低其中的安全限制。

### 2. 统一控制面

1. 盘点并复用 `runtime_config` 的 thermal profile `preview`、`save`、`clear_preview`、`clear_saved` 操作，以及 `calibration_job` 的 `start`/`cancel` 与 `thermal_plant_auto` kind。
2. 使用单一 typed adapter 穿过 Web、LAN/serial transport、devd 和固件；客户端预校验只改善反馈，服务端与固件校验仍是最终裁决。
3. 每一次写操作后读取 status，显示并校验 `thermalControlProfilePreview`、`profileActive`、`profileSource`、resolved bank、target coverage 和 effective parameters；禁止以本地表单值代替设备事实。

### 3. 完成 Web 工作流

1. 在设备上下文中显示热控 profile mode、resolved bank、有效来源、供电/安全资格和锁定原因。
2. 提供点位参数的编辑与预览，明确区分未保存 preview、持久化 saved profile 与默认 profile；离开页面或失败时不得将未确认的本地状态显示为已应用。
3. 为 physical thermal plant calibration 和 PID/profile tuning 提供不同的操作入口、运行中状态、取消路径、终态说明及结果入口。
4. 将原始错误码映射为可执行的 UI 状态，但保留具体失败原因；没有有效供电或安全许可时，控制台必须保持可读而禁用加热相关写操作。

### 4. 保证真实调优路径

1. 在固件和 control-plane 测试中证明：preview 的有效 point-local 参数被选入目标温度的实际控制循环，而不是仅记录在报告中。
2. 覆盖 warmup、approach、hold 和 fault/abort 等阶段，证明候选的 effective parameters 决定实际控制行为，而非只存在于报告中；任何输出限幅都必须来自现行安全、供电或 profile 契约，且可从 status/telemetry 解释。
3. 对 thermal plant calibration 保持现有受保护的满功率瞬态、切断和被动冷却行为；不得将其误实现为可随意 preview 的 PID profile 操作。
4. 对离线 `thermal retune`、preview 应用和报告生成，保持候选、审查快照和 EEPROM saved profile 的语义分离。

### 5. 失败、取消与持久化

1. 供电合同丢失、过温、传感器/EEPROM 故障、校准失败或取消时，heater 必须安全停用；UI 和报告显示事实终态，不得掩盖保护故障。
2. preview 失败、状态读回不一致或作业失败时不得保存 profile；失败后的 RAM/EEPROM 状态必须由测试明确覆盖。
3. 成功保存必须经过 EEPROM 既有原子提交和读回语义；thermal plant 标定的原子提交与 PID profile save 不得互相覆盖。

### 6. 调优报告契约锁定

PID/profile 调优报告是本交付的正式审查证据，不是可随意裁剪的 UI 摘要。本节锁定为 `thermal-tuning-v2`；它取代 legacy preliminary bundle 作为新调优的验收格式。实现不得修改、弱化、改名或以替代产物绕过下列约束。

1. CLI 的 `flux-purr thermal tune --engine firmware` 与 Web 各自独立生成同构的 `thermal-tuning-v2` 报告。`host-reference` 可以生成带 `engine=host-reference` 的诊断/比对报告，但不能声称设备接受了它的候选。历史 `scripts/thermal_tuning*`、`thermal_self_test_preliminary_bundle` 和只含 `thermal-profile.accepted.json` 的 legacy bundle 可以为查阅而重渲染，却不能补造为 v2 完整报告、候选升格或 EEPROM 保存证据。
2. 每次 run 都固定产出五个同一 `runId` 的 UTF-8 文件，即使失败或 `review_incomplete` 也不得缺失：`index.html`（离线可渲染的人读报告）、`run.bundle.json`（结构化元数据和文件 digest）、`samples.ndjson`（全部 sample 事件）、`thermal-profile.candidate.json`（canonical candidate 与 promotion receipts）和 `decision-ledger.ndjson`（全部非 sample 事件）。报告不得再以 `thermal-profile.accepted.json` 表示新候选；该历史文件仅可作为导入兼容输入。
3. `run.bundle.json` 的顶层必须包含：`schema=thermal-tuning-v2`、`runId`、`engine`、设备/固件 identity、`powerClass`、`ppsControlPolicyVersion`、`ppsVoltageBounds`（absolute min/max、policy floor、APDO min/max）、完整物理目标集、所选目标集、实际执行顺序、qualification scope、`terminalDisposition`、`reviewDisposition`、`trace`、`candidate`、`referenceComparison`、file digests 与 capability metadata。`trace` 必须给出 `firstSequence`、`lastSequence`、`complete`、`digest` 和 `gap`；候选必须给出 `candidateId`、`candidateHash` 与 promotion state。
4. trace 是从零开始、全局单调、无重复的事件序列。`samples.ndjson` 与 `decision-ledger.ndjson` 合并后，在 `trace.complete=true` 时必须覆盖 `firstSequence..lastSequence` 的每一个 sequence。允许的事件种类固定为 `sample`、`phase_transition`、`candidate_trial`、`decision` 和 `safety`。任何 sequence 缺口、digest 不符、持久化失败或被覆盖的未确认事件必须显式成为 `trace_gap/review_incomplete`，不得由相邻样本推断、插值或静默删除。
   `trace.digest` 使用跨目标稳定的 `thermal-tuning-event-digest-v2` 规范：事件 JSON 中的 `decision.score` 不依赖平台浮点文本格式，而以固定 `1e-6` 精度的有符号整数参与链哈希；其它事件字段按固件 canonical serialization 参与哈希。主机和设备必须使用同一规范校验，不能以重新格式化的展示 JSON 替代原始事件链。
5. 每个 sample 必须记录：`sequence`、`elapsedMs`、`kind`、`targetC`、`trialIndex`、candidate identity/reference、`temperatureCentiC`、`vinMv`、`ppsContractMv`、`ppsContractMa`、`ppsContractKind`、`ppsContractConfirmed`、`ppsRequestedMv`、`ppsPowerCeilingMv`、`heaterRequestedOutputPermille`、`heaterOutputPermille`、测量有效性和 core/PID phase。`ppsPowerCeilingMv` 虽沿用该字段名，含义仅为所选 APDO 的生产 PPS 请求电压上限，单位 mV；它不得由合同电压、电流或 `V × I` 推导，也不是实测功率。`heaterRequestedOutputPermille` 是 PPS 选择和安全限幅前的 PID 逻辑请求；`heaterOutputPermille` 是功率映射、软启动和本地安全限幅后最后写入加热 MOS 的 PWM duty。二者均为 `0..=1000`、必须来自独立 trace 来源、不得相互复制或由展示层推导。
6. 每个 `candidate_trial` 必须携带完整 canonical candidate 参数、candidate hash、trial 起止 sequence/时间和样本范围。参数不得以默认值掩盖，必须覆盖所有影响加热输出的 point-local 字段：warmup power/re-entry、brake、approach power/floor/damping/tail、hold power/reheat、entry/exit/on/off/cutoff、PI、lead 与 blend；还必须记录 candidate seed 来源及当前 run 的上下边界。每个 `decision` 必须保留完整 score vector、全部 hard-gate 结果、freeze、interval prune、disposition 和失败原因；PPS scheduler decision 还必须能复盘稳定窗口、逻辑请求/实际 PWM 均值、调压前后请求、方向、policy version 和 `floor_limited` 事实。
7. candidate-ready 必须同时证明：该 candidate 自己的 warmup 控制器请求为 `100%`、存在完整 `approach -> hold`、`maxOvershootC <= 3.0`、`holdPeakToPeakC <= 3.0`、满足既有 full-speed-to-stable 动态门槛并完成 `60s` hold confirm。报告必须分别显示 candidate 的 warmup、approach、hold、稳定窗口建立用时和各自门槛；前一 candidate 或 cooldown 的样本不得计入后续 candidate 的评分、预热或指标。
8. `thermal-profile.candidate.json` 必须保存 canonical profile、bank、`runId`、candidate ID/hash、promotion state 和 post-seal receipts。preview receipt 必须含 RAM applied hash；save receipt 必须含匹配的 EEPROM bank revision/hash。preview/save 的身份必须严格绑定 `runId + candidateId + candidateHash + powerClass`；只有 seal 后成功 preview 且经第二次简单确认的同一候选可以 save。`terminalDisposition` 和 `reviewDisposition` 保留完整事实；面向操作者的资格结论只能显示 `passed` 或 `failed`，任何 incomplete、取消、预算耗尽、安全故障或保存/readback 不匹配都不得被显示为通过。
9. 报告页面的目标卡、验收指标、candidate 详情和默认主图必须引用同一个 adopted trial，并显示试验编号/总数。其余 trial 必须可切换，显示 `rejected`、`passed` 或 `adopted`、过冲、峰峰值和 gate。主温度、控制和设备电气图随试验切换；不得叠加不同 trial 或把 rejected 轨迹归因于 adopted 卡。`cooldown_wait` 保留在原始 trace 审计中，但不占用主响应图。温度、控制输出、VIN/PPS 电压和 PPS 合同电流必须使用清晰的独立量纲；控制图同时展示 `logicalHeaterOutputPercent` 与 `mosPwmDutyPercent`，两者均为 `0..100%`，不允许隐藏倍率、`0..1` 归一化值或负值坐标。
10. 正式报告只保留可由设备或 host recorder 真实获得的设备本地证据。外部 VBUS/source 的实测电压、电流和功率被明确排除，不能成为必需字段、缺失提示、验收替代物或伪造的 source telemetry；PPS 合同电流只能标示安全 ceiling，不能称为外部实测电流。每个缺失的必需设备字段必须结构化标记为 unavailable。
11. `thermal_plant_auto` 与 PID tuning 报告不得混淆。前者只报告其单次受保护瞬态、220C cut-off、80C passive cooldown 与原子 active 写入事实；它不生成 PID candidate、v2 decision ledger、preview-to-save 或 PID acceptance report。

### 7. 验证和分阶段 HIL

1. 先完成 host 侧 firmware、devd、Web 单元/集成、mock transport、报告和浏览器证据。
2. 所有 UI 修改必须按项目视觉证据流程在桌面和移动视口验证，涵盖可用、锁定、运行、失败和完成状态。
3. 软件验证通过后，才可建立独立 HIL 执行记录。HIL 前必须由主人给出精确 MCU 端口以及必要的烧录/串口授权；端口消失或重枚举时立即停止，不能自动改用新端口。

## 验收标准

| 编号 | 可验证结果 | 最低证据 |
| --- | --- | --- |
| AC-01 | 交付分支从锁定的 `origin/main` 基线建立；主干若在相关路径前进，须先研判合同差异。热控交付不包含未经授权的 PD 协议实现改动。 | `git merge-base HEAD origin/main`、基线 commit、主干后续相关路径 diff、交付变更列表和 review。 |
| AC-02 | 控制台能显示设备实际的 thermal profile mode、resolved bank、profile source、preview/saved 状态、供电资格和锁定原因。 | adapter/component tests；真实或 mock status fixture；桌面与移动视觉证据。 |
| AC-03 | 合法 profile 可 preview，status 读回证明其在 RAM 中生效；clear preview 不写 EEPROM。 | control-plane/devd/firmware tests 覆盖请求、读回和重启语义。 |
| AC-04 | 合法 profile 只有经 save 后才持久化；clear saved 不错误清除独立的 RAM preview；非法 bank、枚举、点位或范围被明确拒绝。 | runtime config validation 与 EEPROM contract tests。 |
| AC-05 | `thermal_plant_auto` 与 PID/profile tuning 在 UI、请求、终态和持久化语义上可区分；前者按现有单次受保护标定原子生效，后者遵循 preview/review/save 流程。 | job/control-plane tests 与流程级浏览器测试。 |
| AC-06 | 调优候选的 effective parameters 实际驱动目标热控循环；warmup/approach/hold 轨迹、输出和报告不再受替换前控制器的隐式限制。 | firmware orchestration tests、带 telemetry 的 mock/integration run、候选参数与运行时参数比对。 |
| AC-07 | 无有效 `PowerState`、合同过期或任一安全锁时，加热写操作被拒绝或安全停用，且控制台仍显示诊断信息。 | fault/contract-loss tests；UI locked-state evidence。 |
| AC-08 | 取消、故障、EEPROM 写入/读回失败和 status mismatch 都不会把候选误报为已保存或已通过；报告保留可复盘的失败事实。 | negative-path tests、报告 fixture 和持久化断言。 |
| AC-09 | 修改后的 Web、firmware 和 devd 通过与变更面匹配的质量门。 | `bun run check:web`、`bun run check:web:build`、`bun run check:storybook`、`bun run check:firmware:fmt`、相关 firmware/devd tests，以及 `git diff --check`。重型或全量验证按共享测试机流程执行。 |
| AC-10 | 真实热端验收只在明确硬件授权后进行，并以主题 SPEC 的温度、过冲、稳定性、功率资格、故障和报告要求判定。 | 授权端口记录、HIL 原始遥测与 canonical report bundle；软件通过本身不替代此项。 |
| AC-11 | 调优报告满足本合同第 6 节的五文件、数据完整性、展示、结果分类、指标和持久化语义；完整报告的 sequence/digest 覆盖全部 sample 与 decision 事件，不完整报告明确 `trace_gap/review_incomplete`，且不存在 candidate/saved profile 混淆或失败事实被隐藏。 | complete 与 review-incomplete canonical bundle fixtures、报告渲染/解析测试、schema assertions 和人工 review。 |
| AC-12 | 3A 在相同模型/电源/温度输入下保持 plant PI 输出、`Pmax(T)` 饱和与安全结果；5A 正常加热与调优使用同一 profile 控制器，preview/save/clear 的有效 point/settings 可改变可观测逻辑请求和最终 MOS 输出；有效模型门禁、PD/PPS、电流、传感器、过温和标定保护保持有效。5A 不要求与旧 plant PI 逐值同输出，电源类别不得在 armed 状态热切换。 | 3A 基线 fixture、5A 正常/调优路径和候选参数差异测试、故障/合同丢失/源类别切换测试、执行器边界断言及授权 HIL。 |

## 关键经验

- 将协议时限放在拥有协议状态的服务中：PD 的 5 ms cadence 不能借用 50 ms 加热 tick；加热只能消费稳定、带有效期的状态投影。
- 将“候选被生成”“候选被选择”“候选实际控制热端”和“候选被持久化”视为四个不同事实，每个事实都需要独立读回或测试证据。
- 一次物理 thermal plant 标定与 PID profile 调优的风险、持久化和用户语义不同，不能为简化 UI 而混合。
- 先隔离并关闭独立问题。PD、显示启动、烧录和热控调优同时修改会破坏可归因性，也会让 HIL 结果失去基线。
- 真机行为需要真机证据；host build、mock 和报告生成只能证明软件路径。设备权限与端口纪律是验收前提，不是事后补充。
- UI 需要准确呈现设备状态，而不是乐观回显本地操作。对受保护硬件来说，状态读回是写操作的一部分。

## 完成定义与未授权边界

软件阶段完成的条件是 AC-01 至 AC-09、AC-11 和 AC-12 全部满足，并且所有变更已对照本合同和上位规范完成 review。AC-10 是单独的硬件验收门：在主人未提供精确端口和相应操作授权前，它保持未执行，不得以任何模拟结果替代。

固件 60°C 冷却起点及风扇边界由 [独立交付合同](FIRMWARE-60C-COOLDOWN-CONTRACT.md) 限定。条件延长的计时、有限上限和原确认/保存流程由 [整改合同](FIRMWARE-TUNING-REMEDIATION-CONTRACT.md#计时上限与候选处置) 锁定；有限延长量是需确定并验证的实现参数，长期 SPEC 与实现同步，不作为额外主人决策。其余 PD 协议、生产算法或热端物理验收阈值变更仍须单独研判。
