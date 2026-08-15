# Flux Purr Web 固件安装与恢复实现状态

> 当前有效规范以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 事实。

## Current Status

- Implementation: 进行中
- Lifecycle: active
- Delivery: fast-track PR，停在 Step 5C merge-ready

## Coverage / rollout summary

- 合同与 fixtures 已冻结，源码实现按 bundle/firmware、devd、Web/UI 分片推进。
- 真实串口和全擦 HIL 尚未获精确端口授权，不属于当前非硬件验证。

## Remaining Gaps

- 完成 M1-M5 后更新为实际覆盖和验证命令。

## References

- `./SPEC.md`
- `./HISTORY.md`

