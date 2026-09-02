# Flux Purr 蜂鸣器单输出 Cue 仲裁主题历史

> 本文件记录本主题的局部生命周期、兼容性与必要背景；完整取舍见 `docs/adr/0006-single-output-buzzer-cue-arbitration.md`。

## Lifecycle / Compatibility

- 本主题为 active，补充现有前面板和热失控 attention 合同，不替代它们的输入或安全状态真相源。
- GPIO48 维持单一 passive-buzzer PWM 输出；仲裁不改变既有 cue 的内部音阶或 PWM 载波契约。

## Replacements / Background

- 旧播放边界允许任意后到 Cue Request 直接替换当前播放，因而无法保证一个 cue 的完整性。
- 已有的载波保持修复只解决静音间隙的 PWM 重配置，不定义请求优先级或反馈合并。

## Related Changes

- [`../../adr/0006-single-output-buzzer-cue-arbitration.md`](../../adr/0006-single-output-buzzer-cue-arbitration.md)

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
