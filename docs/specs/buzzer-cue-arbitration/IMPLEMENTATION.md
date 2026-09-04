# Flux Purr 蜂鸣器单输出 Cue 仲裁实现状态

> 当前有效需求以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 事实。

## Current Status

- Implementation: 已完成
- Lifecycle: active
- Catalog note: 单一 PWM 蜂鸣器的请求仲裁合同由固件内部唯一仲裁边界执行。

## Implementation Coverage

- `REQ-BUZZER-ARBITRATION-001` 至 `REQ-BUZZER-ARBITRATION-008`：`firmware/src/buzzer.rs` 中的 `BuzzerArbiter` 是唯一的 cue 选择边界；模块私有的 `BuzzerController` 仅负责已选 cue 的 tone/rest 步骤。`run_buzzer_task` 在 priority-2 software-interrupt executor 中独占二者、MCPWM0 timer2/operator2 与 GPIO48 duty 写入，业务主循环只提交有界请求；因此显示、USB 和控制循环的协作式 poll 不会推迟 cue step。
- `BuzzerArbiter` 以 `ProtectionAlarm > AttentionReminder > FeedbackCue` 仲裁请求，保留一个 Pending Feedback，并在 `tick` 中报告延后启动的 cue。
- `ProtectionAlarm` 保留既有 `2300Hz`、静音、`2300Hz`、静音四步音型，作为 `300ms` non-looping one-shot；thermal scheduler 在活动热失控期间每秒请求重放。
- 启动恢复、热失控状态机、两个 cadence scheduler、runtime-control 和前面板反馈均使用仲裁请求，并用来源、cue 与 disposition 记录固件诊断。
- real-time 时序任务按 cue step deadline 唤醒并输出 GPIO48 duty；迟到时控制器只前进一个 step 并从实际输出时刻重新计时，因此短 rest 不会被一次补偿性 tick 吞掉。静音仍是 duty 归零，同频载波仍被复用。Timer2 使用固定 prescaler `3`，异频 step 先静音并停止 Timer2，再将计数器归零、应用匹配目标音高的 period、重启 Timer2，最后恢复目标 duty。
- `buzzer-test` 是默认正式测试 feature。它经 native USB JSONL 与 `devd` lease 向同一 real-time 时序任务提交生产 `BuzzerCueId` 或固定仲裁场景，返回最多八条会话决策 trace。可选 `buzzer-observe` 在同一输出所有权内增加最多十六条输出 trace；每条输出 trace 同时保留逻辑请求频率、由 Timer2 配置寄存器推导的载波、GPIO48 pad 经 PCNT 上升沿计数得到的观测频率、duty 与 cue generation。普通 Feedback Cue 连续模式在每轮生产音型结束后重新通过 `BuzzerArbiter` 提交；`ProtectionAlarm` 调用生产 `ProtectionAlarmCadence`，以相同的一秒节奏重放；`AttentionReminder` 保持其十秒节奏。两种 feature build 都使用普通固件的启动、GPIO48/PWM 输出和 real-time 时序任务，不含独立恢复播放或原始 PWM 控制；只有 `buzzer-observe` 初始化 PCNT 探针。LAN 与产品 Web 控制面没有此端点。

## Coverage / rollout summary

- 通过 host-side 仲裁单元测试、feature-gated USB/devd 协议测试、固件运行时测试和 PWM 载波回归测试验证；生产构建不新增 Control Plane API、持久化字段或设备配置。
- 授权设备上的 `active_cooling_on` 连续播放以 GPIO48 PCNT 闭环观测到对应 `900 / 1200 / 1550Hz` 的三段载波，并完成听感确认。

## Remaining Gaps

- 无阻塞本主题交付的实现缺口。

## Related Changes

- [`../../adr/0006-single-output-buzzer-cue-arbitration.md`](../../adr/0006-single-output-buzzer-cue-arbitration.md)

## References

- `./SPEC.md`
- `./HISTORY.md`
