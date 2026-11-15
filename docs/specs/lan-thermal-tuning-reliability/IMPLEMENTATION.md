# LAN 设备会话与固件热控调优读取 实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、验证和剩余缺口。

## Current Status

- Implementation: 固件缺省基线与 Web/devd 调优启动路径已覆盖；LAN 会话迁移、完整 owner proof 和浏览器证据仍待收口。
- Lifecycle: active
- Catalog note: Web LAN session 与 tuning GET/POST 已有基础实现，但能力声明、错误分类和轮询收敛尚未完整符合主题契约。

## Implementation Coverage

- `REQ-LTR-001`: `web/src/features/control-plane-demo/lan-client.ts` 已能依据稳定 `deviceId` 合并 LAN 目标并更新地址；凭据存储仍以地址记录为主，需要迁移为设备身份归属并保留已验证地址。
- `REQ-LTR-002`: `lan-pairing-panel.tsx` 已有已保存凭据恢复和 401 重新配对入口；需要确保调优读取失败不会删除凭据或打开配对面板。
- `REQ-LTR-003`: `control-plane-demo.tsx` 已把调优读取放入独立回调，但 `FirmwareTuningWorkspace` 仍需要将 tuning read state 与顶部连接状态明确隔离。
- `REQ-LTR-004`: `firmware/src/net_http.rs` 已注册 `/api/v1/thermal-tuning` GET/POST；`firmware/src/control_plane.rs` 当前只在 `web_serial` 条件下声明 `thermal_tuning`，需要补齐 `net_http` 能力契约和稳定 unsupported 响应。
- `REQ-LTR-005`: LAN client 已有无租约 GET 回归测试，调优 POST 仍沿用 lease；固件网关已按 HTTP method 区分只读和写入租约。
- `REQ-LTR-006`: `lan-client.ts` 当前把所有 `TypeError` 映射为 `lan_transport_unavailable`，错误文案仍可能把调优失败解释成 LAN 设备无法连接，需要改为端点级 typed error。
- `REQ-LTR-007`: `firmware-tuning-workspace.tsx` 已在三次可重试失败后停止轮询并提供手动重试动作。
- `REQ-LTR-008`: 当前错误显示复用通用 `messageOf`，需要加入调优操作、响应类别和安全动作的中文消息目录。
- `REQ-LTR-009`: 全局日志已有设备事件面板，但调优读取尝试尚未统一记录 endpoint、结果类别和 retry count；敏感字段必须继续保持 redaction。
- `REQ-LTR-010`: Web Serial、devd 和 direct LAN 已有独立 adapter；需要使用同一 tuning domain state 和 fixture matrix 验证 transport parity。
- `REQ-LTR-012`: `thermal_tuning.rs` 已编译进 `pps3a` 与 `pps5a` 两份完整工厂缺省 profile；无上传候选时使用匹配基线并保留独立安全门禁。
- `REQ-LTR-013`: 启动时设备候选优先、缺省时工厂基线兜底；候选仍保持 RAM 预览并按功率档位保存。
- `REQ-LTR-014`: devd 调优写请求继续受租约保护，停止请求作为已认证安全例外允许无租约执行；单运行和保存 owner proof 仍需后续跨层收口。

## Verification / Rollout

- 已有 LAN client 测试覆盖 read-only thermal tuning GET 不携带 lease，以及基础传输错误映射。
- 已有 firmware route 和 Web Serial thermal tuning 测试覆盖命令解析与状态读取基础路径。
- 本主题的完整验证必须补充 `VER-LTR-001` 至 `VER-LTR-019` 对应的 fixture、组件测试、固件契约检查和本地浏览器验证；不得使用线上设备或线上版本代替本地验证。

## Remaining Gaps

- 按设备身份保存和恢复 LAN pairing credential，并在地址变化时只更新已验证 locator。
- 为 `net_http` 固件补齐 `thermal_tuning` capability 与 `thermal_tuning_unsupported` error envelope。
- 拆分 tuning read state 与 top-level connection state。
- 将 `TypeError`、HTTP 状态和协议错误映射为端点级错误。
- 完善 owner proof、跨客户端写入禁用和本地浏览器验收证据。
- 为 status 成功、tuning 失败的组合场景补充本地 fixture 和 UI 验收。
- 为 `pps3a` 与 `pps5a` 补充独立工厂数据审计记录和功率档位选择门禁测试。
- 在 firmware、devd、Web Serial 和 LAN adapter 中统一实现 `thermal_parameters_unavailable`、候选预览、确认保存、回滚和跨客户端停止语义。

## Related Changes

- [ADR 0027: Embedded factory thermal baselines before tuning](../../adr/0027-embedded-factory-thermal-baselines.md)

## References

- `./SPEC.md`
- `./HISTORY.md`
