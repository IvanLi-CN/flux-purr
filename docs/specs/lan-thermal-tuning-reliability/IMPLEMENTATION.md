# LAN 设备会话与固件热控调优读取 实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、验证和剩余缺口。

## Current Status

- Implementation: 固件缺省基线与 Web/devd 调优启动路径已覆盖；LAN 会话迁移、完整 owner proof 和浏览器证据仍待收口。
- 电源状态语义已明确：空闲时固定 5V/1A 属于正常安全状态；只有调优运行意图存在时才触发 PPS 协商。PPS 未确认前固件不加热、不采样，Web 应显示电源合同等待或失败原因，不得显示设备连接失败。
- 显式开始调优会建立待恢复的加热意图；实际输出仍须等待已确认的 PPS 合同，并继续通过传感器、过温、commissioning 和持久化安全门禁。调优进入 warmup 后，只有这些门禁全部满足才会驱动加热器。
- Lifecycle: active
- Catalog note: Web LAN session 与 tuning GET/POST 已有基础实现，但能力声明、错误分类和轮询收敛尚未完整符合主题契约。

## Implementation Coverage

- `REQ-LTR-001`: `web/src/features/control-plane-demo/lan-client.ts` 已能依据稳定 `deviceId` 合并 LAN 目标并更新地址；凭据存储仍以地址记录为主，需要迁移为设备身份归属并保留已验证地址。
- `REQ-LTR-002`: `lan-pairing-panel.tsx` 已有已保存凭据恢复和 401 重新配对入口；需要确保调优读取失败不会删除凭据或打开配对面板。
- `REQ-LTR-003`: `control-plane-demo.tsx` 已把调优读取放入独立回调，但 `FirmwareTuningWorkspace` 仍需要将 tuning read state 与顶部连接状态明确隔离。
- `REQ-LTR-004`: `firmware/src/net_http.rs` 已注册 `/api/v1/thermal-tuning` GET/POST；`firmware/src/control_plane.rs` 当前只在 `web_serial` 条件下声明 `thermal_tuning`，需要补齐 `net_http` 能力契约和稳定 unsupported 响应。
- `REQ-LTR-005`: LAN client 已有无租约 GET 回归测试，调优 POST 仍沿用 lease；固件网关已按 HTTP method 区分只读和写入租约。
- `REQ-LTR-006`: `lan-client.ts` 当前把所有 `TypeError` 映射为 `lan_transport_unavailable`，错误文案仍可能把调优失败解释成 LAN 设备无法连接，需要改为端点级 typed error。
- `REQ-LTR-007`: `firmware-tuning-workspace.tsx` 的空闲和活动运行共用有限读取预算；连续三次可重试失败后保留最后快照与游标，停止轮询并提供手动重试动作。活动运行不得自动清零失败计数。对最后观察到的活动运行，安全停止不依赖读取重新确认或本地日志就绪；其他写入仍受原有门禁保护。
- `REQ-LTR-008`: 当前错误显示复用通用 `messageOf`，需要加入调优操作、响应类别和安全动作的中文消息目录。
- `REQ-LTR-009`: 全局日志已有设备事件面板，但调优读取尝试尚未统一记录 endpoint、结果类别和 retry count；敏感字段必须继续保持 redaction。
- `REQ-LTR-010`: Web Serial、devd 和 direct LAN 已有独立 adapter；需要使用同一 tuning domain state 和 fixture matrix 验证 transport parity。
- 调优使用生产 `AutomaticThermal` 电源请求。高优先级 PD 服务为同一已确认 APDO、同一合同电流内的健康 PPS 电压步进保留 heater permit，避免低优先级控制周期恢复输出前被异步强制关断；能力刷新、合同范围/电流变化、拒绝、超时和低压互锁仍关断。预热实际全输出资格不放宽，零输出帧继续使该试验不合格。
- 预热 PPS 工作点保留一个 100mV 上限量化档位的余量；仍安全的已确认合同允许在这个余量内保持，避免 RTD 量化边界抖动导致后端反复主动降压、等待稳定和零输出。真正越界的合同仍立即关断并迁入安全范围，物理全输出资格保持不变。
- devd 活动会话和调优请求绑定页面选中的具体 bridge 目标，不得在显示 LAN 时借用同设备 USB 目标的租约。调优 GET 在 Web、devd USB 和 devd LAN 间透传 `run_id` 与完整宽度 `after_sequence`，包括零值；缺少游标会重复返回旧页、无法确认已收事件，最终耗尽设备事件缓冲并使运行失败。
- `REQ-LTR-012`: `thermal_tuning.rs` 已编译进 `pps3a` 与 `pps5a` 两份完整工厂缺省 profile；无上传候选时使用匹配基线并保留独立安全门禁。
- `REQ-LTR-013`: 启动时设备候选优先、缺省时工厂基线兜底；候选仍保持 RAM 预览并按功率档位保存。
- `REQ-LTR-014`: devd 调优写请求继续受租约保护，停止请求作为已认证安全例外允许无租约执行；单运行和保存 owner proof 仍需后续跨层收口。

## Verification / Rollout

- 当前工作树的完整功能 Xtensa release 可编译；host 固件测试分别为 671 项与 601 项通过。目标专属分支的构造字段与 defmt 参数类型必须通过显式 `--target xtensa-esp32s3-none-elf` 构建检查，host 构建不提供此覆盖。
- 完整功能 Xtensa release 与无网络检查产物均通过栈门禁。完整功能镜像的嵌套 PD 中断余量为 98,460B（下限 65,536B），USB 控制链余量为 67,020B、LAN 控制链余量为 51,004B（下限均为 32,768B）。EEPROM 读取域记录以堆分配数组传递，写入域记录按需逐份构造，避免大枚举数组的栈副本；域选择、写入顺序、readback 和提交标记语义保持不变。
- Rust structural check（含 14 项 checker fixtures）、host firmware Clippy 与 Xtensa Clippy 已通过。Web 相关单测 61 项、Biome 全量检查、typecheck 和 production build，以及 devd 测试已通过；这些是源码和构建证据，不替代当前设备镜像的 HTTP 完整调优验收。Web 读取 effect 显式依赖读取状态，操作组使用具名 fieldset 并重置浏览器默认边框、间距和最小宽度。
- 离线 factory-app 镜像大小检查统一通过 `just firmware-image-check` 执行，完整功能镜像为 2,079,168B / 2,097,152B，已通过分区大小检查。当前 Candidate 的真机完整调优、浏览器证据及 PR current-head CI/review 仍未收敛。
- `web/e2e/firmware-tuning-polling.spec.ts` 通过真实 React 组件与浏览器 IndexedDB 验证：成功读取活动运行后，三次失败停读并保留快照；手动重试只开启一轮有限读取；停止按钮保持可用并将对应 runId 的 `cancel` 传入 transport 回调。fixture 不连接 devd、设备或外部服务，不能替代真机验收。停止按钮在读取重新确认期间被错误禁用的问题已由该测试复现并验证修复。
- Web 经 devd LAN HTTP 的运行 `web-pps3a-9711ee99-0fb9-4a43-9a62-440ec540a055` 完整收取 809 条事件，游标连续推进，未再次耗尽事件缓冲；该运行仍失败，首个 60°C 试验在健康 PPS 调压附近记录 5 帧逻辑 100% / 实际 0%，被物理全输出资格拒绝。高优先级许可遗漏 `AutomaticThermal` 已由回归测试复现，修复后 host 测试通过；匹配固件复测仍失败。后续定点诊断确认零输出由后端主动关断和 settle 窗口产生，期间 PD permit 有效、供电响应故障未锁存；100mV 上限往返的回归测试复现了该问题。电压余量与安全合同保持的修复通过后，仍须取得新的完整真机调优证据。
- Web HTTP 运行 `web-pps3a-5a44b5b5-5bf9-4b64-ae51-c30250e4412a` 在约 367.3 秒由 `host_cancel` 结束，事件范围为 0..1313；该运行未完成八目标资格。其烧录 ELF 在最后的恢复逻辑编辑前已生成，不能证明当前工作树的固件修复有效，也不能作为当前 Candidate 的通过证据。必须在目标产物安全检查通过后重新烧录并取得匹配产物身份的完整验证。

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
