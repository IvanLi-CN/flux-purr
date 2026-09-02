# Flux Purr 蜂鸣器单输出 Cue 仲裁

> 本文是蜂鸣器 cue 仲裁的长期需求合同。当前实现覆盖见 `IMPLEMENTATION.md`，主题局部历史见 `HISTORY.md`。

## Context and Scope

- Context: `GPIO48` 上的单一 PWM 输出只能在任一时刻驱动一个 Buzzer Cue；多个独立请求必须经确定性仲裁后才可到达该输出。
- In scope: cue 请求的优先级、抢占、单槽合并、安全状态抑制、调度语义与诊断边界。
- Out of scope: cue 的具体音高/时长设计、蜂鸣器硬件拓扑、PWM 载波生成、Control Plane 外部 API、测温 fault 的安全策略。

## Terms and Interfaces

- `Buzzer Cue`、`Cue Request`、`Cue Arbitration`、`Protection Cue`、`Attention Reminder`、`Feedback Cue`、`Pending Feedback` 与 `Audible Safety State`：见根目录 `CONTEXT.md`。
- Interface: 固件内部的 cue 仲裁边界接收携带语义来源的 Cue Request，并返回 `selected`、`preempted`、`queued`、`coalesced` 或 `dropped` 的仲裁结果。
- Interface: 仅 cue 仲裁边界可启动或停止底层播放控制器；底层控制器继续输出已选 cue 的 tone/rest 步骤。

## Requirements

### REQ-BUZZER-ARBITRATION-001

- 系统 MUST 在单一 cue 仲裁边界中选择 GPIO48 的唯一活动 cue；任何业务调用方不得直接替换底层正在播放的 cue。
- Inputs: 前面板、运行时控制和 thermal attention 发出的 Cue Request，及其语义来源。
- Outputs: 单一活动 cue、至多一个 Pending Feedback，以及可诊断的仲裁结果。

### REQ-BUZZER-ARBITRATION-002

- 系统 MUST 以 `ProtectionAlarm > AttentionReminder > FeedbackCue` 的优先级仲裁请求。
- `ProtectionAlarm` MUST 保留其既有内部 tone/rest 模式，但作为 non-looping one-shot 在热失控进入时立即播放，并在活动热失控期间以一秒节奏重播。
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

### REQ-BUZZER-ARBITRATION-007

- 仲裁层 MUST 为每个 Cue Request 产生带语义来源、请求 cue 和仲裁结果的固件诊断记录。
- 诊断记录 MUST 不新增 Control Plane 产品 API、持久化状态或凭据表面。

## Verification

### VER-BUZZER-ARBITRATION-001

- Method: 确定性的 host-side 仲裁单元测试。
- covers: `REQ-BUZZER-ARBITRATION-001`
- Pass condition: 重叠请求序列在每个时刻只有一个活动 cue，且业务请求不能绕过仲裁边界替换底层播放状态。

### VER-BUZZER-ARBITRATION-002

- Method: 覆盖活动 `ProtectionAlarm` 与普通 cue 请求的确定性时序测试。
- covers: `REQ-BUZZER-ARBITRATION-002`
- Pass condition: 普通 cue 请求不会改变保护 cue 的后续步骤；保护 cue 保持 one-shot 且按一秒节奏重新开始。

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

- Method: 既有 cue 步骤与 PWM 载波复用回归测试。
- covers: `REQ-BUZZER-ARBITRATION-006`
- Pass condition: cue 内部频率序列保持，静音阶段只关闭 duty，相同下一频率不重配载波。

### VER-BUZZER-ARBITRATION-007

- Method: 固件诊断记录断言。
- covers: `REQ-BUZZER-ARBITRATION-007`
- Pass condition: 每个仲裁结果均包含来源、cue 和 disposition，且没有新的对外或持久化字段。

## Related ADRs

- [Single-output buzzer cue arbitration](../../adr/0006-single-output-buzzer-cue-arbitration.md)

## Visual Evidence

- None

## References

- `./IMPLEMENTATION.md`
- `./HISTORY.md`
- [`../q2aw6-heater-pid-frontpanel-runtime/SPEC.md`](../q2aw6-heater-pid-frontpanel-runtime/SPEC.md)
- [`../fk3u7-frontpanel-input-interaction/SPEC.md`](../fk3u7-frontpanel-input-interaction/SPEC.md)
