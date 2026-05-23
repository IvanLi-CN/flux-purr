# Flux Purr 热控 Bench Web Demo 实现状态（#hhwq8）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现
- Lifecycle: active
- Catalog note: React/Vite fixed bench console + Storybook mock UI demo

## Coverage / rollout summary

- `ControlPlaneDemo` 已作为 `web/src/App.tsx` 第一屏。
- `web/src/features/control-plane-demo/**` 提供 scenario types、deterministic mock data 与工业风固定控制台界面。
- `web/src/stories/ControlPlaneDemo.stories.tsx` 覆盖 default、degraded、settings review、update review、gallery、mobile review 与 interaction smoke。
- 工业风 token 与组件样式集中在 `web/src/index.css` 的 `.industrial-*` class；当前 UI 去掉后台式侧边栏、指标墙和长滚动内容，提供 Dashboard / Settings / Update 与桌面全局日志，不改变 frontpanel preview 渲染器。
- Dashboard 当前温度为首要信息，目标温度 stepper 放在主操作行内；变更立即写入 mock runtime 与 trace。
- Settings 已收敛为 heat policy：live summary、preset slot grid、preset temperature debounce autosave、preset enabled switch 与 fan policy segmented control。
- Update 已收敛为 firmware check：artifact selector、compatibility verdict、dry-check progress 与 blocked/warning/success mock 状态。
- 全局日志使用 `@tanstack/react-virtual` + `simplebar-react` 渲染 1000 条 deterministic trace；滚动条只在 hover/滚动时显示，follow-tail 由用户显式切换。
- 移动端保留轻量 trace ticker 和单列内容，避免用完整日志面板挤压核心热控操作。

## Remaining Gaps

- PR 号在 PR 创建后回填。

## Related Changes

- None

## References

- `./SPEC.md`
- `./HISTORY.md`
- `../../solutions/device-control/web-native-wifi-bridge-console.md`
