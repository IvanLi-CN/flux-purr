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

## Related Changes

- None

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- [`../../adr/0026-separate-device-identity-network-locator-and-operation-readiness.md`](../../adr/0026-separate-device-identity-network-locator-and-operation-readiness.md)
