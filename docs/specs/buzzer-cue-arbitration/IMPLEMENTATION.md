# Flux Purr 蜂鸣器单输出 Cue 仲裁实现状态

> 当前有效需求以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 事实。

## Current Status

- Implementation: 未开始
- Lifecycle: active
- Catalog note: 单一 PWM 蜂鸣器的请求仲裁合同已冻结。

## Implementation Coverage

- `REQ-BUZZER-ARBITRATION-001` 至 `REQ-BUZZER-ARBITRATION-007`：尚无仲裁器实现或对应回归覆盖。
- 现有底层 `BuzzerController`、GPIO48 PWM 输出和载波复用逻辑是可复用的输出层，但现有调用方仍直接请求播放，不能满足仲裁合同。
- 既有 `ProtectionAlarm` 仍是循环模式；转换为 cadence-owned one-shot 属于待实现行为。

## Coverage / rollout summary

- 本主题当前只冻结合同与验证边界，未修改固件、Control Plane、设备配置或硬件状态。
- 真机确认需要主人提供精确授权端口；不得以发现到的候选端口替代授权目标。

## Remaining Gaps

- 固件内部仲裁边界、来源标记和单槽 Pending Feedback 状态。
- 保护与 reminder 调度接入仲裁边界，并保留既有确认安全语义。
- 面向仲裁和运行时 attention 路径的确定性回归测试与诊断记录验证。
- 在授权设备上的只读声音/诊断关联确认。

## Related Changes

- [`../../adr/0006-single-output-buzzer-cue-arbitration.md`](../../adr/0006-single-output-buzzer-cue-arbitration.md)

## References

- `./SPEC.md`
- `./HISTORY.md`
