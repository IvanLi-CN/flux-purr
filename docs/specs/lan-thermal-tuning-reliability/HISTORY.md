# LAN 设备会话与固件热控调优读取 主题历史

> 这里记录主题生命周期、兼容性和决策背景；长期取舍保留在 `docs/adr/`。

## Lifecycle / Compatibility

- 主题状态为 `active`。
- 本主题补充并细化 `real-control-plane-runtime` 对 LAN 目标恢复、能力声明和 lease boundary 的要求，不取代该主题的 firmware、devd 或整体控制面契约。
- 旧固件若没有 `thermal_tuning` capability 或稳定 unsupported 响应，客户端只能报告可证实的通用端点失败，并不得伪造“设备未连接”。兼容性收敛后，旧固件应通过明确 unsupported 分支进入只读调优页面。

## Replacements / Background

- 用户观察到设备顶部显示已连接、实时温度可读，但固件热控调优区域反复显示无法连接 LAN 设备；该现象暴露出连接状态、端点读取和写入租约被错误合并。
- 设备身份与网络定位地址被明确拆分：IP 变化只改变请求定位，不自动触发重新配对；只有设备身份验证失败或设备明确拒绝 bearer 才进入重新配对流程。
- 固件热控调优 GET 被确定为只读操作，POST/保存继续要求控制租约；前端轮询必须有界，不能用无限重试掩盖端点级错误。
- 调优启动规则进一步收敛为：固件内置 `pps3a` 和 `pps5a` 两份完整工厂缺省热控基线；设备标定数据优先，缺少两类参数来源时禁止加热型调优；未知供电档位不猜测。
- 调优候选只在 RAM 中生效，确认保存后才写入对应功率档位；同一设备同时只允许一个调优运行，其他客户端禁止写入但可以读取和停止。

## Related Changes

- [ADR 0027: Embedded factory thermal baselines before tuning](../../adr/0027-embedded-factory-thermal-baselines.md)

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- [`../../adr/0026-separate-device-identity-network-locator-and-operation-readiness.md`](../../adr/0026-separate-device-identity-network-locator-and-operation-readiness.md)
