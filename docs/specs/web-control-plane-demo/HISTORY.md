# Flux Purr 热控 Bench Web Demo 演进历史

## Legacy identity

- Former legacy ID: `hhwq8`.

## Lifecycle

- `active`: the mock-first demo remains the canonical demo surface.

> 这里记录会影响 Agent 理解“为什么一步步变成现在这样”的关键演进；单次任务流水账不放这里，规范正文仍以 `./SPEC.md` 为准。

## Decision Trace

- `#27` 合并后，控制平面架构先以 solution 形式存在，覆盖 Web UI、native daemon、USB CDC、WiFi provisioning、firmware flashing 与 monitoring。
- Web demo 选择先做 mock-first 轻工具，而不是直接实现 daemon 或 firmware transport，因为 UX、信息架构和 guardrails 可以在无硬件条件下稳定验证。
- 初版界面一度接近管理后台；随后收敛为固定高度 bench console，设备切换降为顶部辅助状态栏控件，主空间改为 Dashboard / Settings / Update 三个界面，并在桌面端保留同时可见的全局日志。
- Dashboard 的主语义从 transport/WiFi handoff 修正为 Flux Purr 热板运行状态：当前温度、目标温度、heater 输出、PD 合约与风扇/主动降温是首层信息；WiFi 与连接只作为辅助上下文或 Settings 内的低优先级配置能力。
- 设计审查后继续压低 header 与普通面板的视觉权重；Settings 必须像设置而不是状态摘要，Update 必须先给出明确 gate verdict，移动端日志只能作为摘要 ticker 而不是完整日志面板。
- Dashboard 目标温度调整被放回主操作行，并改为实时生效；Settings 的 preset 温度改为 debounce 自动保存，不再提供 Apply 或 Use as target 这类额外提交动作。
- Preset slot 增加启用/禁用状态，并用 UI library switch 表达；disabled slot 仍可选择编辑，但视觉层级低于可用 preset。
- 全局日志从少量静态行改为 1000 条 mock trace 的虚拟列表；follow-tail 不再默认强制滚到底部，滚动条仅在 hover/滚动时出现。
- Demo 使用独立 `control-plane-demo` feature，避免把轻量连接工具与既有 `160×50` 前面板 preview contract 混在一起。
- 生产导航采用 TanStack Router file-based routing：设备稳定 identity 位于一级路径，工作台 view 与校准 workspace 位于二级路径；浏览器 history、刷新和分享链接因此使用同一状态源。
- Storybook 保留无 router adapter 的本地状态模式，生产应用则通过 `ConsoleNavigationAdapter` 把同一控制台组件受控化，避免复制视觉与业务实现。
- `demo` 与 `uiDemo` 保留为 typed search，而不是组件内模式 state；`uiDemo` 只服务 production 根入口，Inspector state 随站内导航保留，EdgeOne 通过 SPA rewrite 承接 history 深链。
- transport target id、设备地址与凭据不具备跨连接稳定性，因此规范 URL 只接受探针验证后的物理 identity；无法恢复时保留地址并显示恢复操作。
- 校准离开保护迁入 router blocker，使 Link、程序导航、设备切换、search 变化和浏览器历史共享“先退出校准、成功后继续”的安全顺序。
- 公开 Demo 复用正式控制台和路由，而非替换 `uiDemo` LAN pairing surface；Inspector 只控制确定性 fixture state，并通过同一 search blocker 接入校准离开保护。
- EdgeOne public Demo 使用独立 Makers artifact 与项目绑定，避免把 `flux-purr-demo.ivanli.cc` 误作 live direct-LAN origin。

## Key Reasons / Replacements

- 工业拟物风格用于表达硬件工具的物理可靠性，但信息架构必须保持轻，不能扩张成 fleet 管理后台，也不能通过长滚动堆复杂度。
- Storybook 是 Web UI 的稳定视觉证据来源，优先于临时浏览器窗口截图。
- 路由视觉证据使用 mock-only `uiDemo` 与离线 identity 恢复态，不以真实设备状态或真实硬件截图作为验收输入。
- `docs/solutions/device-control/web-native-wifi-bridge-console.md` 继续作为跨任务复用经验；本 spec 只冻结 Flux Purr 当前 demo surface。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- `../../solutions/device-control/web-native-wifi-bridge-console.md`
