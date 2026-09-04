# Flux Purr 热控 Bench Web Demo 实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现
- Lifecycle: active
- Catalog note: React/Vite fixed bench console + TanStack Router + Storybook mock UI demo

## Coverage / rollout summary

- `web/src/main.tsx` 以 TanStack `RouterProvider` 挂载生产应用；Vite file-based routing 插件生成并提交 `web/src/routeTree.gen.ts`。
- pathless console layout 在 `web/src/App.tsx` 把 leaf match 转换为 `ConsoleNavigationAdapter`，生产导航由 URL 驱动；Storybook 不传 adapter 时继续使用本地 view state。
- 设备页面使用 `/devices/:deviceId/{overview,settings,update}`，校准页面使用 `/devices/:deviceId/calibration/{heater-curve,rtd-adc,vin-adc}`，连接入口使用 `/devices/new`。
- root search validator 将 `demo` 与 `uiDemo` 规范化为类型化 boolean，并通过 search middleware 在 Link 与程序导航中保留 `demo` 和 Inspector state；`uiDemo` 保持 production 根入口专用。
- `flux-purr.routePreferences.v1` 只保存 variant 最近成功的稳定 identity 与该 identity 最近成功的 transport kind；transport target id、地址与凭据不进入 URL。
- 深链恢复按 transport 偏好和健康候选串行解析；Web Serial 自动恢复只读取已授权端口，只有显式添加操作可以调用 chooser。
- 未知 identity 保留原 URL 并显示重试、选择设备和添加连接操作；结构无效的 URL 由 root 404 处理。
- 校准运行期间的 Link、程序导航、设备切换、search 变化与浏览器 Back/Forward 统一进入 TanStack blocker resolver；armed 状态同时注册 `beforeunload`。
- `web/public/edgeone.json` 仅将 `/devices` history 深链 rewrite 到 `/index.html`，让 `/assets/*` 按文件路径提供。
- `build:demo` 使用 Vite demo mode 输出 `web/dist-demo`；它固定为 fixture runtime、关闭 devd/Web Serial，并把 root replace 到 `fp-lab-01` overview。
- `DemoInspector` 作为控制台同级的可收起工具层，使用 `demoScene`、`demoLease`、`demoNetwork` 与 `demoArtifact` 复现确定性状态；高级状态只读可复制，面板布局不进入 URL。
- `Release Product` 将独立的版本化 public-demo archive 发布到 GitHub Release，并只在发布资产验证完成后部署该 archive 到 `flux-purr-demo`；release marker 使 recovery 不会重复部署。
- `web/src/features/control-plane-demo/**` 提供 scenario types、deterministic mock data 与工业风固定控制台界面。
- `web/src/stories/ControlPlaneDemo.stories.tsx` 覆盖 default、degraded、settings review、update review、gallery、mobile review 与 interaction smoke。
- 工业风 token 与组件样式集中在 `web/src/index.css` 的 `.industrial-*` class；当前 UI 提供 Dashboard / Settings / Calibration / Update 与桌面全局日志，不改变 frontpanel preview 渲染器。
- Dashboard 当前温度为首要信息，目标温度 stepper 放在主操作行内；变更立即写入 mock runtime 与 trace。
- Settings 已收敛为 heat policy：live summary、preset slot grid、preset temperature debounce autosave、preset enabled switch 与 fan policy segmented control。
- Update 已收敛为 firmware check：artifact selector、compatibility verdict、dry-check progress 与 blocked/warning/success mock 状态。
- 全局日志使用 `@tanstack/react-virtual` + `simplebar-react` 渲染 1000 条 deterministic trace；滚动条只在 hover/滚动时显示，follow-tail 由用户显式切换。
- 移动端保留轻量 trace ticker 和单列内容，避免用完整日志面板挤压核心热控操作。

## Remaining Gaps

- PR 号在 PR 创建后回填。
- merge 与 production release 不属于当前实现状态。

## Related Changes

- None

## References

- `./SPEC.md`
- `./HISTORY.md`
- `../../solutions/device-control/web-native-wifi-bridge-console.md`
