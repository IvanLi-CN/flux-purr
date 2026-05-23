# Flux Purr 热控 Bench Web Demo 演进历史（#hhwq8）

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

## Key Reasons / Replacements

- 工业拟物风格用于表达硬件工具的物理可靠性，但信息架构必须保持轻，不能扩张成 fleet 管理后台，也不能通过长滚动堆复杂度。
- Storybook 是 Web UI 的稳定视觉证据来源，优先于临时浏览器窗口截图。
- `docs/solutions/device-control/web-native-wifi-bridge-console.md` 继续作为跨任务复用经验；本 spec 只冻结 Flux Purr 当前 demo surface。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- `../../solutions/device-control/web-native-wifi-bridge-console.md`
