# Flux Purr 蜂鸣器单输出 Cue 仲裁

> 本文是蜂鸣器 cue 仲裁的长期需求合同。当前实现覆盖见 `IMPLEMENTATION.md`，主题局部历史见 `HISTORY.md`。

## Context and Scope

- Context: `GPIO48` 上的单一 PWM 输出只能在任一时刻驱动一个 Buzzer Cue；多个独立请求必须经确定性仲裁后才可到达该输出。
- In scope: cue 请求的优先级、抢占、单槽合并、安全状态抑制、调度语义、已选 cue 的 GPIO48 输出切换约束与诊断边界，以及 feature-gated 的开发蜂鸣器诊断。
- Out of scope: cue 的具体音高/时长设计、蜂鸣器硬件拓扑、产品 Control Plane API、原始 PWM 参数控制、测温 fault 的安全策略。

## Terms and Interfaces

- `Buzzer Cue`、`Cue Request`、`Cue Arbitration`、`Protection Cue`、`Attention Reminder`、`Feedback Cue`、`Pending Feedback`、`Audible Safety State` 与 `Developer Buzzer Diagnostic`：见根目录 `CONTEXT.md`。
- Interface: 固件内部的 cue 仲裁边界接收携带语义来源的 Cue Request，并返回 `selected`、`preempted`、`queued`、`coalesced` 或 `dropped` 的仲裁结果。
- Interface: priority-2 software-interrupt executor 中的专用蜂鸣器时序任务是 `BuzzerArbiter`、底层播放控制器和 GPIO48 PWM 写入的唯一拥有者；业务模块只提交 Cue Request。

## Requirements

### REQ-BUZZER-ARBITRATION-001

- 系统 MUST 在单一 cue 仲裁边界中选择 GPIO48 的唯一活动 cue；任何业务调用方不得直接替换底层正在播放的 cue。
- 专用蜂鸣器时序任务 MUST 独占仲裁器、cue 步进和 GPIO48 duty 写入；它 MUST 运行在独立于业务主循环的 priority-2 software-interrupt executor 中，并在 cue step deadline 或 Cue Request 到达时运行；业务主循环不得推进 cue 或直接写 GPIO48。
- Inputs: 前面板、运行时控制和 thermal attention 发出的 Cue Request，及其语义来源。
- Outputs: 单一活动 cue、至多一个 Pending Feedback，以及可诊断的仲裁结果。

### REQ-BUZZER-ARBITRATION-002

- 系统 MUST 以 `ProtectionAlarm > AttentionReminder > FeedbackCue` 的优先级仲裁请求。
- `ProtectionAlarm` MUST 保留其既有四步音型（`2300Hz`、静音、`2300Hz`、静音）并作为 non-looping one-shot 在热失控进入时立即播放；活动热失控期间以一秒节奏重播。
- 活动 `ProtectionAlarm` MUST 立即抢占较低优先级 cue，并清除 Pending Feedback。

### REQ-BUZZER-ARBITRATION-003

- `AttentionReminder` MUST 在已清除但未确认的热失控状态下保持十秒提醒节奏。
- 当 `AttentionReminder` 到期时，系统 MUST 允许已开始的 Feedback Cue 正常收尾；随后 `AttentionReminder` MUST 先于任何 Pending Feedback 播放，并替换该 Pending Feedback。
- `AttentionReminder` MUST 被新的 `ProtectionAlarm` 立即抢占。

### REQ-BUZZER-ARBITRATION-004

- Feedback Cue MUST 不截断当前活动 cue。
- 系统 MUST 最多保留一个 Pending Feedback：重复 `ui_input` 请求合并为一次；专用状态或拒绝 cue 替换 Pending `ui_input`；连续专用 cue 以最新状态替换旧 Pending Feedback。
- 每个真正被选中开始播放的 cue MUST 从其第一步开始；被合并或丢弃的请求不得重启当前 cue。

### REQ-BUZZER-ARBITRATION-005

- Audible Safety State MUST 抑制所有 Feedback Cue 请求，不得把它们写入 Pending Feedback。
- 进入、离开或确认 Audible Safety State MUST 清除 Pending Feedback；被抑制或清除的反馈不得在安全状态之后补播。
- 前面板确认 `faultAttentionPending` 状态的既有“确认/静音且不执行原操作”语义 MUST 保持。

### REQ-BUZZER-ARBITRATION-006

- 仲裁层 MUST 不改变已选 cue 内部的 tone/rest 顺序。
- 底层 PWM 输出 MUST 继续保持 boot/idle 静音、GPIO48 独占，以及跨 duty-zero 静音间隙复用相同频率载波的既有契约。
- Timer2 MUST 使用固定 prescaler，并通过适配目标频率的 period 表示每个生产 cue 音高；不得以运行时切换 prescaler 作为音高控制。下一有声 step 与当前载波频率不同时，底层输出 MUST 先将 GPIO48 duty 归零，再停止 Timer2、将计数器归零、应用新 period 并重启 Timer2，最后才恢复目标 duty。相同频率的静音间隙和重放 MUST 不停表或重调。
- 时序任务迟到时 MUST 先输出尚未实际写入的下一 step；它不得在一次 tick 中跳过 tone/rest step，尤其不得丢失短静音间隙。

### REQ-BUZZER-ARBITRATION-007

- 仲裁层 MUST 为每个 Cue Request 产生带语义来源、请求 cue 和仲裁结果的固件诊断记录。
- 生产固件与产品 Control Plane MUST 不新增蜂鸣器诊断 API、持久化状态或凭据表面。

### REQ-BUZZER-ARBITRATION-008

- 开发固件在显式启用 `buzzer-debug` feature 后，MUST 通过 native USB JSONL 与受 lease 保护的 `devd` 端点提供 `Developer Buzzer Diagnostic`；能力必须由 `buzzer_debug` identity capability 明确声明，且不得经 LAN 或产品 Web 控制面暴露。
- 该诊断 MUST 只接受生产 `BuzzerCueId` 或固定 `feedback_coalesce` / `feedback_replace` / `active_cooling_retrigger` 仲裁场景，并且仍 MUST 通过 `BuzzerArbiter` 提交。`ProtectionAlarm` MUST 复用 production `ProtectionAlarmCadence` 和已存在的安全请求接口；`AttentionReminder` MUST 复用其十秒 cadence。它不得暴露频率、占空比、原始步骤或持久化控制。
- 诊断触发 MUST 在加热、测温 fault、热保护 latch 或未确认的 thermal attention 存在时拒绝；返回的有限 decision trace MUST 只记录本诊断会话的仲裁结果。调试 build 还 MUST 返回有限 output trace：每一项关联已请求的 cue 输出、Timer2 `prescaler` / `period` 推导的配置 carrier、GPIO48 pad 经 PCNT 上升沿计数得到的观测 carrier、duty 与 generation，且只在同一 real-time 时序任务实际应用输出后记录。PCNT 只读回该 pad 的数字波形，不得表述为声学频率测量。静音项的逻辑频率为 `null`，但 timer 配置按普通固件的 duty-zero 复用合同保留。普通 Feedback Cue 的显式 `repeat` MUST 在每轮生产音型结束后重新通过 `BuzzerArbiter` 提交；保护和提醒 cue MUST 保持生产 cadence。重复播放只能由显式 `repeat` 请求开始，并且 MUST 由显式 `stop` 请求结束。
- `buzzer-debug` feature MUST 在标准运行时初始化、主循环、GPIO48 输出应用与 cue pattern 之上添加该受控测试会话；它不得使用恢复模式、替代传感器输入、替代 heater/GPIO 初始化或独立 PWM 路径。

## Verification

### VER-BUZZER-ARBITRATION-001

- Method: 确定性的 host-side 仲裁单元测试。
- covers: `REQ-BUZZER-ARBITRATION-001`
- Pass condition: 重叠请求序列在每个时刻只有一个活动 cue，且业务请求不能绕过仲裁边界替换底层播放状态。

### VER-BUZZER-ARBITRATION-002

- Method: 覆盖活动 `ProtectionAlarm` 与普通 cue 请求的确定性时序测试。
- covers: `REQ-BUZZER-ARBITRATION-002`
- Pass condition: 普通 cue 请求不会改变保护 cue 的后续步骤；保护 cue 保持固定载波 one-shot 且按一秒节奏重新开始。

### VER-BUZZER-ARBITRATION-003

- Method: 覆盖 Feedback Cue、到期 `AttentionReminder` 和随后 `ProtectionAlarm` 的确定性时序测试。
- covers: `REQ-BUZZER-ARBITRATION-003`
- Pass condition: Feedback Cue 完整结束后才开始 reminder，reminder 替换等待中的反馈，新的保护 cue 立即抢占 reminder。

### VER-BUZZER-ARBITRATION-004

- Method: 覆盖重复 `ui_input`、专用状态 cue 和拒绝 cue 的合并测试。
- covers: `REQ-BUZZER-ARBITRATION-004`
- Pass condition: 当前反馈不重启；Pending Feedback 始终至多一个，且保存最后一个有意义的反馈。

### VER-BUZZER-ARBITRATION-005

- Method: 覆盖热失控进入、清除和确认的运行时状态测试。
- covers: `REQ-BUZZER-ARBITRATION-005`
- Pass condition: 安全状态中的 Feedback Cue 被丢弃，安全状态转换后不存在可播放的旧 Pending Feedback，确认输入不触发其原本操作。

### VER-BUZZER-ARBITRATION-006

- Method: cue step deadline、迟到 tick 与 PWM 载波复用回归测试。
- covers: `REQ-BUZZER-ARBITRATION-006`
- Pass condition: cue 内部频率序列保持，迟到 tick 仍依次输出 tone/rest；Timer2 保持固定 prescaler，异频阶段按 `duty 0 -> stop timer -> reset/apply period/start -> target duty` 执行，相同下一频率只切换 duty 而不重配载波。

### VER-BUZZER-ARBITRATION-007

- Method: 固件诊断记录断言。
- covers: `REQ-BUZZER-ARBITRATION-007`
- Pass condition: 每个仲裁结果均包含来源、cue 和 disposition，且生产固件没有新的对外或持久化字段。

### VER-BUZZER-ARBITRATION-008

- Method: feature-gated 固件 USB frame、`devd` request 验证测试与授权设备 GPIO48 PCNT 闭环验证。
- covers: `REQ-BUZZER-ARBITRATION-008`
- Pass condition: 只有声明 capability 的开发固件可接收 production cue/scenario 请求；普通 cue 连续模式会在每轮结束后重新通过仲裁，`protection_alarm --repeat` 与运行时共用一秒 cadence；请求无法携带原始 PWM 参数，并在真实热安全 interlock 存在时被拒绝；多音 cue 的 GPIO48 pad 观测频率分别跟随其目标音高。

## Related ADRs

- [Single-output buzzer cue arbitration](../../adr/0006-single-output-buzzer-cue-arbitration.md)

## Visual Evidence

- None

## References

- `./IMPLEMENTATION.md`
- `./HISTORY.md`
- [`../q2aw6-heater-pid-frontpanel-runtime/SPEC.md`](../q2aw6-heater-pid-frontpanel-runtime/SPEC.md)
- [`../fk3u7-frontpanel-input-interaction/SPEC.md`](../fk3u7-frontpanel-input-interaction/SPEC.md)
