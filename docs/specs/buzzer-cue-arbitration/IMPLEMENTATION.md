# Flux Purr 蜂鸣器单输出 Cue 仲裁实现状态

> 当前有效需求以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 事实。

## Current Status

- Implementation: 已完成
- Lifecycle: active
- Catalog note: 单一 PWM 蜂鸣器的请求仲裁合同由固件内部唯一仲裁边界执行。

## Implementation Coverage

- `REQ-BUZZER-ARBITRATION-001` 至 `REQ-BUZZER-ARBITRATION-008`：`firmware/src/buzzer.rs` 中的 `BuzzerArbiter` 是唯一的 cue 选择边界；模块私有的 `BuzzerController` 仅负责已选 cue 的 tone/rest 步骤。`run_buzzer_task` 在 priority-2 software-interrupt executor 中独占二者、MCPWM0 timer2/operator2 与 GPIO48 duty 写入，业务主循环只提交有界请求；因此显示、USB 和控制循环的协作式 poll 不会推迟 cue step。
- `BuzzerArbiter` 以 `ProtectionAlarm > AttentionReminder > FeedbackCue` 仲裁请求，保留一个 Pending Feedback，并在 `tick` 中报告延后启动的 cue。
- `ProtectionAlarm` 保留既有四步节奏并使用固定 `2300Hz` 载波，作为 `300ms` non-looping one-shot；thermal scheduler 在活动热失控期间每秒请求重放。
- 启动恢复、热失控状态机、两个 cadence scheduler、runtime-control 和前面板反馈均使用仲裁请求，并用来源、cue 与 disposition 记录固件诊断。
- real-time 时序任务按 cue step deadline 唤醒并输出 GPIO48 duty；迟到时控制器只前进一个 step 并从实际输出时刻重新计时，因此短 rest 不会被一次补偿性 tick 吞掉。静音仍是 duty 归零，同频载波仍被复用。
- `buzzer-debug` 是非默认开发 feature。它经 native USB JSONL 与 `devd` lease 向同一 real-time 时序任务提交生产 `BuzzerCueId` 或固定仲裁场景，返回最多八条会话决策 trace，并返回最多十六条 MCPWM timer2 输出 readback trace。每条输出 readback 同时保留逻辑请求频率、由 timer 寄存器 prescaler/period 推导的实际载波、duty 与 cue generation，因此可证明 cue 选择与 GPIO48 的实际 timer 配置一致。`ProtectionAlarm` 测试调用生产 `ProtectionAlarmCadence`，以相同的一秒节奏经 `BuzzerArbiter` 重放；`AttentionReminder` 保持其十秒节奏。feature build 使用普通固件的启动、GPIO48/PWM 输出和 real-time 时序任务，不含独立恢复播放或原始 PWM 控制。生产构建不声明此 capability，LAN 与产品 Web 控制面也没有此端点。

## Coverage / rollout summary

- 通过 host-side 仲裁单元测试、feature-gated USB/devd 协议测试、固件运行时测试和 PWM 载波回归测试验证；生产构建不新增 Control Plane API、持久化字段或设备配置。
- 真机确认不在本主题的 host-side 验收范围内。若未来需要，必须由主人提供精确授权端口；不得以发现到的候选端口替代授权目标。

## Remaining Gaps

- 无阻塞本主题交付的实现缺口。
- 授权设备上的只读声音/诊断关联确认可作为后续硬件验证，但不替代或阻塞本主题的 host-side 合同验证。

## Related Changes

- [`../../adr/0006-single-output-buzzer-cue-arbitration.md`](../../adr/0006-single-output-buzzer-cue-arbitration.md)

## References

- `./SPEC.md`
- `./HISTORY.md`
