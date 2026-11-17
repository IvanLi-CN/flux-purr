# LAN 设备会话与固件热控调优读取 实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、验证和剩余缺口。

## Current Status

- Implementation: 固件缺省基线与 Web/devd 调优启动路径已覆盖；LAN 身份存储和端点错误已有实现；完整 owner proof、跨客户端门禁和浏览器验收仍待收口。
- 电源状态语义已明确：空闲时固定 5V/1A 属于正常安全状态；只有调优运行意图存在时才触发 PPS 协商。PPS 未确认前固件不加热、不采样，Web 应显示电源合同等待或失败原因，不得显示设备连接失败。
- 显式开始调优会建立待恢复的加热意图；实际输出仍须等待已确认的 PPS 合同，并继续通过传感器、过温、commissioning 和持久化安全门禁。调优进入 warmup 后，只有这些门禁全部满足才会驱动加热器。
- Web 固定 5V 提示说明这是正常空闲供电，启动后固件自动协商 PPS；未确认 PPS 只能说明暂时不加热，不应解释为无法发起调优。
- Lifecycle: active
- Catalog note: Web LAN session、tuning GET/POST、能力声明与有限读取已有实现；跨 adapter 错误一致性与完整运行验收尚未收敛。

## Implementation Coverage

- `REQ-LTR-001`: `web/src/features/control-plane-demo/lan-client.ts` 已能依据稳定 `deviceId` 合并 LAN 目标并更新地址；`storeLanDeviceSession` 按身份键保存并兼容旧地址记录；`resumeLanDeviceSession` 在匿名 health 身份匹配后复用凭据并迁移 locator，身份不符不发送 bearer。
- `REQ-LTR-002`: `lan-pairing-panel.tsx` 已有已保存凭据恢复和 401 重新配对入口；需要确保调优读取失败不会删除凭据或打开配对面板。
- `REQ-LTR-003`: `control-plane-demo.tsx` 已把调优读取放入独立回调；`FirmwareTuningWorkspace` 使用自己的读取状态、快照和重试预算；真机已观察到顶部温度持续更新而调优读取停止，完整 fixture 矩阵仍需验收。
- `REQ-LTR-004`: `firmware/src/net_http.rs` 已注册 `/api/v1/thermal-tuning` GET/POST；`firmware/src/control_plane.rs` 在 `web_serial` 或 `net_http` 条件下声明 `thermal_tuning`；Web 和 devd 将调优路由的 404 归一化为 `thermal_tuning_unsupported`。
- `REQ-LTR-005`: LAN client 已有无租约 GET 回归测试，调优 start/seal/confirm-save POST 沿用 lease，已认证 cancel 除外；固件网关区分只读、普通写入和安全停止。
- `REQ-LTR-006`: `lan-client.ts` 与 `transport-client.ts` 已携带 operation、endpoint、HTTP status 和 category，区分 401、调优 404、5xx、无效 JSON 与传输异常；devd 的传输消息仍可能直接显示底层英文，需要端点级中文提示。
- `REQ-LTR-007`: `firmware-tuning-workspace.tsx` 的空闲和活动运行共用有限读取预算；连续三次可重试失败后保留最后快照与游标，停止轮询并提供手动重试动作。活动运行不得自动清零失败计数。对最后观察到的活动运行，安全停止不依赖读取重新确认或本地日志就绪；其他写入仍受原有门禁保护。
- `REQ-LTR-008`: 当前错误显示复用通用 `messageOf`，需要加入调优操作、响应类别和安全动作的中文消息目录。
- `REQ-LTR-009`: 全局日志已有设备事件面板，但调优读取尝试尚未统一记录 endpoint、结果类别和 retry count；敏感字段必须继续保持 redaction。
- `REQ-LTR-010`: Web Serial、devd 和 direct LAN 已有独立 adapter；需要使用同一 tuning domain state 和 fixture matrix 验证 transport parity。
- 调优使用生产 `AutomaticThermal` 电源请求。高优先级 PD 服务为同一已确认 APDO、同一合同电流内的健康 PPS 电压步进保留 heater permit，避免低优先级控制周期恢复输出前被异步强制关断；能力刷新、合同范围/电流变化、拒绝、超时和低压互锁仍关断。预热实际全输出资格不放宽，零输出帧继续使该试验不合格。
- 自动 PPS 的预热、接近、保持与供电准备工作点持续保留一个 100mV 上限量化档位的余量；合同保持分支与 Hold governor 使用同一工作上限，不能消耗该余量而重新贴住硬安全上限。健康的小步调压保持输出，真正越界的合同仍立即关断并迁入安全范围，物理全输出资格保持不变。确认合同位于实时上限下方一个 100mV 档位内时即可进入预热，不要求精确等于上限。
- 已确认的 PPS 合同可能领先后端缓存的请求值；重同步是否关断须依据实时安全电压上限，不能把低于当前合同的目标请求值当成安全上限。模式变化和实际越界仍关断；同一安全范围内的重同步保持物理输出。
- devd 活动会话和调优请求绑定页面选中的具体 bridge 目标，不得在显示 LAN 时借用同设备 USB 目标的租约。调优 GET 在 Web、devd USB 和 devd LAN 间透传 `run_id` 与完整宽度 `after_sequence`，包括零值；缺少游标会重复返回旧页、无法确认已收事件，最终耗尽设备事件缓冲并使运行失败。
- HTTP socket 回收必须结束连接两端后再放回监听池。`close()` 后的 `flush()` 只等待本端 FIN 确认，不能证明对端已关闭；处于 `FIN_WAIT_2` 的 socket 无法重新 `accept()`。响应正文完整发送并获得 TCP 确认后直接 abort 并有界等待 reset 发出，不为对端 half-close 占用有限 socket，避免并发轮询期间失去监听能力。只读回归入口为 `just lan-http-read-check <device-origin> <device-id>`，不携带凭据、不跟随重定向，身份不符立即停止。
- devd 对同一规范化 HTTP origin 的已认证请求共享两个在途槽，其余请求公平等待，等待超过 3 秒返回可重试的 `503 server_retryable`。这是请求调度预算，不限制客户端数量或取得控制权；不同 origin 使用独立预算。写入前用于读取 revision 的 status 响应必须完整收取，再发送写入，避免丢弃正文占用设备连接。八个并发 HTTP 读取的回归测试在限流前超过两个在途请求，限流后通过。
- `REQ-LTR-012`: `thermal_tuning.rs` 已编译进 `pps3a` 与 `pps5a` 两份控制器起点 profile；无上传候选时可以解析匹配起点，并保留独立安全门禁。输出模型、热模型初值、安全边界与工厂数据来源的完整性审计尚未完成，不能将控制器 profile 等同于完整工厂基线验收。
- `REQ-LTR-013`: 启动时设备候选优先、缺省时工厂基线兜底；候选仍保持 RAM 预览并按功率档位保存。
- `REQ-LTR-014`: 调优停止在固件 HTTP 网关、排队命令执行、devd LAN bridge 与 direct LAN Web 中作为已认证安全例外执行，不获取 mutation lease、不读取 optimistic revision；重复停止保持已停机状态和原有终态证据。start、seal、confirm-save 和其它写入继续受原有租约与 revision 保护；单运行和保存 owner proof 仍需后续跨层收口。
- 活动运行在固件共同 USB/LAN 命令分发处拒绝客户端写入并返回 `thermal_tuning_active`；只读和调优安全停止仍允许。判定覆盖运行配置、Wi-Fi、标定、持久化、加热曲线、其它调优写入和蜂鸣器写入；蜂鸣器状态读取仍允许。devd 将该 USB 协议错误保留为 HTTP 409，避免误报为通信网关故障。Web 全局禁写提示与置灰、匹配镜像的跨客户端实测仍待完成。

## Verification / Rollout

- 活动运行写入策略的 USB wire 矩阵和蜂鸣器只读例外测试通过；devd 响应/错误两类帧的回归测试在修复前观察到 502，修复后均为 409。固件 host 677 项库测试和 607 项运行时测试通过，devd 全量检查、Rust structural check、host 与 Xtensa 严格 Clippy 通过。该源码保护尚未烧录，不作为当前正在运行的镜像证据。
- 当前工作树的完整功能 Xtensa release 可编译；host 固件测试分别为 672 项与 604 项通过。目标专属分支的构造字段与 defmt 参数类型必须通过显式 `--target xtensa-esp32s3-none-elf` 构建检查，host 构建不提供此覆盖。
- 完整功能 Xtensa release 与无网络检查产物均通过栈门禁。完整功能镜像的嵌套 PD 中断余量为 98,460B（下限 65,536B），USB 控制链余量为 67,020B、LAN 控制链余量为 51,004B（下限均为 32,768B）。EEPROM 读取域记录以堆分配数组传递，写入域记录按需逐份构造，避免大枚举数组的栈副本；域选择、写入顺序、readback 和提交标记语义保持不变。
- Rust structural check（含 14 项 checker fixtures）、host firmware Clippy 与 Xtensa Clippy 已通过。Web 相关单测 61 项、Biome 全量检查、typecheck 和 production build，以及 devd 测试已通过；这些是源码和构建证据，不替代当前设备镜像的 HTTP 完整调优验收。Web 读取 effect 显式依赖读取状态，操作组使用具名 fieldset 并重置浏览器默认边框、间距和最小宽度。
- 离线 factory-app 镜像大小检查统一通过 `just firmware-image-check` 执行，完整功能镜像为 2,079,184B / 2,097,152B，已通过分区大小检查。当前 Candidate 的真机完整调优、浏览器证据及 PR current-head CI/review 仍未收敛。
- `web/e2e/firmware-tuning-polling.spec.ts` 通过真实 React 组件与浏览器 IndexedDB 验证：成功读取活动运行后，三次失败停读并保留快照；手动重试只开启一轮有限读取；停止按钮保持可用并将对应 runId 的 `cancel` 传入 transport 回调。fixture 不连接 devd、设备或外部服务，不能替代真机验收。停止按钮在读取重新确认期间被错误禁用的问题已由该测试复现并验证修复。
- Web 经 devd LAN HTTP 的运行 `web-pps3a-9711ee99-0fb9-4a43-9a62-440ec540a055` 完整收取 809 条事件，游标连续推进，未再次耗尽事件缓冲；该运行仍失败，首个 60°C 试验在健康 PPS 调压附近记录 5 帧逻辑 100% / 实际 0%，被物理全输出资格拒绝。高优先级许可遗漏 `AutomaticThermal` 已由回归测试复现，修复后 host 测试通过；匹配固件复测仍失败。后续定点诊断确认零输出由后端主动关断和 settle 窗口产生，期间 PD permit 有效、供电响应故障未锁存；100mV 上限往返的回归测试复现了该问题。电压余量与安全合同保持的修复通过后，仍须取得新的完整真机调优证据。
- Web HTTP 运行 `web-pps3a-5a44b5b5-5bf9-4b64-ae51-c30250e4412a` 在约 367.3 秒由 `host_cancel` 结束，事件范围为 0..1313；该运行未完成八目标资格。其烧录 ELF 在最后的恢复逻辑编辑前已生成，不能证明当前工作树的固件修复有效，也不能作为当前 Candidate 的通过证据。必须在目标产物安全检查通过后重新烧录并取得匹配产物身份的完整验证。
- 固件 `d746a1277a1e404b138fab91c6fa5962a1cade99` 的 Web HTTP 运行 `web-pps3a-cb6d1e73-0f5c-44ca-b543-5b4cbcf7b404` 收取 989 条事件，在 271.4 秒由页面停止；预热连续 118 个样本未出现零输出，运行进入后续候选试验，但 HTTP 读取反复失败，未完成整轮调优。相同镜像的 20 次只读 health 检查出现连接拒绝（19 次成功、1 次失败）；socket 回收修复仍需匹配镜像烧录后的同路径复测。
- 固件 `0dc39f96cd321a73b2c0771d37801ae82287690b` 已经 devd/CLI 烧录到授权端口，ELF SHA-256 为 `b726bda6647d77673c6bfd8c55a9359a37854b6bfd6da6b7df06359a0a71d8ca`；HTTP identity 确认匹配版本，旧凭据保持有效。准备阶段一个 PPS 档位余量的回归测试覆盖已确认、未确认、低于工作点和越过上限的合同。Web 经 devd LAN HTTP 发起运行 `web-pps3a-58ea705a-73cb-4101-91ae-b0886add96ee`，已进入预热和后续候选试验，完整运行结果待观察；高频 health 额外负载的 100 次读取仍有 3 次连接拒绝，不能宣告 HTTP 并发验收通过。烧录按授权跳过 EEPROM 备份，未生成备份归档，EEPROM 健康状态未知。
- devd `9b4540d1` 的 Web HTTP 运行 `web-pps3a-469a34d8-2aee-4724-bad5-dcbac09d45b9` 在约 556 秒后由页面停止，期间同一页面额外发出的八个并发状态读取全部返回 200，调优读取未停轮询。60°C 试验仍被物理全输出资格拒绝，220°C 试验发生预热超时并按既有安全规则从较暖起点重试；这不是完整调优通过证据。
- 固件 `a271705e795bc062cfeba1bc0eae913c259defbc` 已通过 672 项库测试、605 项运行时测试、Xtensa Clippy、2,079,200B 镜像和栈门禁，烧录后 HTTP identity 核验匹配，ELF SHA-256 为 `32d56a52845e1b0b466879a355091465633637797c8d7838aad434ee653acf44`。Web HTTP 运行 `web-pps3a-59426f47-b9d6-45f8-9a16-5dd56605825b` 在 60°C 第三项试验的 Approach → Warmup 转换时再次记录两帧实际零输出（序号 1035、1036，286.318s / 286.439s）；当时 PPS 10.8V 已确认、安全上限 10.9V，温度为 54.72°C / 54.87°C。该试验被全输出资格拒绝，之后 220°C 预热仍未达到目标，运行由页面停止并确认已取消；不构成整轮调优通过证据。

- 已有 LAN client 测试覆盖 read-only thermal tuning GET 不携带 lease，以及基础传输错误映射。
- 已有 firmware route 和 Web Serial thermal tuning 测试覆盖命令解析与状态读取基础路径。
- 本主题的完整验证必须补充 `VER-LTR-001` 至 `VER-LTR-019` 对应的 fixture、组件测试、固件契约检查和本地浏览器验证；不得使用线上设备或线上版本代替本地验证。

- 调优采样在请求非零而实际输出为零时保留 `output_paused` 原因，包括 PWM 关断来源、PD permit、供电响应锁存、settle 窗口以及最后一次 PPS 停热分支和缓存/目标电压。该诊断不修改输出授权、安全门禁或试验资格；`p`、`l`、`s` 依次为 PD 许可、供电锁存和等待窗口状态，`active`、`cap` 保存停热决策时的合同电压与上限；`flags` 的低三位依次代表安全恢复、合同纠正和范围/大步长切换；最后一次停热上下文不等同于当前故障归因，须结合时间与状态核验。

- 固件 `2661678676f9162265fa8cab70621148c8d0c54b` 的 Web HTTP 运行 `web-pps3a-3802e0d8-9b3a-4912-adf9-427481b456bb` 在 42.816 秒记录 `output_paused:command permit=1 latch=0 settle=1 flags=2 cached=10700 next=10800`，证明归零来自 PPS 合同纠正分支而非 PD 许可撤销。Web 停止未在 15 秒内确认；通过同一授权设备的 USB/devd 安全停止接口返回 `armed=false`、`canceled`。该轮未完成，不能用于验收通过。

- 固件 `b813e06b2d89c5a3` 的完整功能镜像经 `just firmware-image-check` 确认为 `2,082,096B / 2,097,152B`，通过栈门禁并烧录至主人授权的精确端口；ELF SHA-256 为 `610184f97d3c42204f6a0de6889204dc281c8dc951e6ed9d2bf6e84e8fe73212`。本地 Web 页面通过 devd LAN bridge 连续两次发送空 lease 的 HTTP `cancel`，均返回 200、`armed=false`、`phase=idle`，旧凭据仍有效。该结果只验证幂等停止，不是完整调优通过证据。烧录按授权跳过 EEPROM 备份，未生成备份归档，EEPROM 健康状态未知。
- HTTP 运行 `web-pps3a-1a8acc31-2c5f-4f39-a57e-e6abebf8ea41` 在 7.443 秒捕获合同 10.0V / 上限 9.9V 导致的输出归零；47.059 秒与 48.309 秒也出现合同纠正停热。436.108 秒时仍处于 220°C 预热、实测 108.86°C，后由页面停止并确认已取消，未完成整轮。工作余量回归测试重放上限上升后回落的序列，修复前明确失败于 `Warmup: 10000 -> 10000 at 10000`；必须取得持续保留余量修复的匹配镜像真机证据。
- 固件 `014d8eb210bfc3f2` 的 Web 经 devd LAN HTTP 运行 `web-pps3a-c8c8884d-95f5-4544-afd7-61a8c9d9c0c9` 在 754 秒前收取 2,664 条事件，预热请求全输出期间未见实际归零；60°C 候选仍未通过接近阶段时间预算。随后连续 HTTP 读取失败耗尽页面重试预算，设备事件缓冲在约 822.9 秒耗尽并进入 `failed / armed=false / review_incomplete`，不是整轮通过。诊断读取确认原 runId 仍存在；补收保留事件后记录 2,919 条事件，其中 2918 因缓冲已满未保留，2919 是诊断中错误超前确认游标产生的 `event_ack_skipped_unconfirmed`，不能当作原始失败原因。停机后 20 次公开 health 读取全部成功，尚不足以排除运行期间的瞬态 HTTP 失败。

## Remaining Gaps

- 对已实现的身份存储、locator 迁移和 401 恢复取得完整跨 adapter fixture 证据。
- 完成 capability 与稳定 unsupported 契约的跨 adapter 验收。
- 完成独立 tuning read state 与顶部连接状态的页面级经验验收。
- 完成端点级错误的 adapter parity 验收，并统一 devd 传输失败的中文提示。
- 完善 owner proof、跨客户端写入禁用和本地浏览器验收证据。
- 为 status 成功、tuning 失败的组合场景补充本地 fixture 和 UI 验收。
- 为 `pps3a` 与 `pps5a` 补充独立工厂数据审计记录和功率档位选择门禁测试。
- 在 firmware、devd、Web Serial 和 LAN adapter 中统一实现 `thermal_parameters_unavailable`、候选预览、确认保存、回滚和跨客户端停止语义。

## Related Changes

- [ADR 0027: Embedded factory thermal baselines before tuning](../../adr/0027-embedded-factory-thermal-baselines.md)

## References

- `./SPEC.md`
- `./HISTORY.md`
