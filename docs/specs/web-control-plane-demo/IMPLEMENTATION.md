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
- `isPublicDemoBuild()` 固定运行时边界：正式构建只挂载 live，公开 Demo 只挂载 mock；旧正式 `demo=true` / `uiDemo` 入口显示停用页，Demo 构建清理旧模式参数。
- root search validator 将 Demo Inspector 的 scene、persona/permissions、目标故障、能力和数据开关规范化为 typed search；规范分享链接只保留这些初始条件与设备/page identity，不保留临时操作。
- `flux-purr.routePreferences.v1` 只保存 variant 最近成功的稳定 identity 与该 identity 最近成功的 transport kind；transport target id、地址与凭据不进入 URL。
- 深链恢复按 transport 偏好和健康候选串行解析；Web Serial 自动恢复只读取已授权端口，只有显式添加操作可以调用 chooser。
- 未知 identity 保留原 URL 并显示重试、选择设备和添加连接操作；结构无效的 URL 由 root 404 处理。
- 校准运行期间的 Link、程序导航、设备切换、search 变化与浏览器 Back/Forward 统一进入 TanStack blocker resolver；armed 状态同时注册 `beforeunload`。
- `web/public/edgeone.json` 仅将 `/devices` history 深链 rewrite 到 `/index.html`，让 `/assets/*` 按文件路径提供。
- `build:demo` 使用 Vite demo mode 输出 `web/dist-demo`；它固定为 fixture runtime、关闭 devd/Web Serial，并把 root replace 到 `fp-lab-01` overview。
- `DemoSession` 由 App 的 reducer-like state 与 typed command 接线组成，产品操作和 Inspector action 共用同一模拟状态；scene、设备、fault、能力/数据开关和 Reset 共享校准离开保护。
- `DemoInspector` 作为控制台同级的可收起工具层，使用 `demoScene`、`demoPersona`、`demoLease`、`demoNetwork`、`demoArtifact`、`demoWifiCapability`、`demoLogs` 与 `demoTelemetry` 复现确定性状态；面板通过按需帮助按钮提供 persona/permissions 与 mock-only 说明，并保留近期 simulated mutation 记录；全局 Demo 设置只承载会话范围的 Persona/权限，当前用例设置承载 scene、target、故障、能力/数据和模拟动作，scene/target 使用项目 Select 组件并显示摘要；Inspector 布局不属于 Demo 状态控制项，只能从同一个收起按钮的右键/触摸长按菜单调整；仅在 1700px+ 且能保留 1280px console 与 gutter 时允许左/右布局，较窄桌面改用占满视口宽度、受控高度的底部布局或悬浮布局，1280px+ 底部布局使用四栏、1180–1279px 使用三栏，各栏独立滚动且页面根层保持固定，不挤压产品口；窄屏使用带 modal 语义的 drawer；收起按钮左键收起，布局菜单点击外部区域或按 Escape 关闭，布局偏好通过 localStorage 保留且不进入 Demo 状态、URL 或分享链接；分享复制等待路由提交并区分 Clipboard 成功/拒绝。
- `web/src/demo-runtime.ts` 在 RouterProvider 之前建立 public demo 的 mock-only fetch boundary；初始化失败渲染安全停止页面，live API 或跨源请求不会回退到真实服务。
- `Release Product` 将独立的版本化 public-demo archive 发布到 GitHub Release，并只在发布资产验证完成后通过共享 helper 部署该 archive 到 `flux-purr-demo`；helper 只对上传或 deployment creation 开始前明确处于 `check and set region` 阶段的 transport-level 失败执行有限重试，输出捕获不完整或远端状态不明确时禁止自动重试，并在部署成功后写入 release marker。Recovery 仍以 marker 作为跳过已标记目标的 guard，不在 marker 上传失败时推断远端部署状态。
- `web/src/features/control-plane-demo/**` 提供 scenario types、deterministic mock data 与工业风固定控制台界面。
- `web/src/stories/ControlPlaneDemo.stories.tsx` 覆盖 default、degraded、settings review、update review、gallery、mobile review 与 interaction smoke。
- 工业风 token 与组件样式集中在 `web/src/index.css` 的 `.industrial-*` class；当前 UI 提供 Dashboard / Settings / Calibration / Update 与桌面全局日志，不改变 frontpanel preview 渲染器。
- Dashboard 当前温度为首要信息，目标温度 stepper 放在主操作行内；变更立即写入 mock runtime 与 trace。
- Settings 已收敛为 heat policy：live summary、preset slot grid、preset temperature debounce autosave、preset enabled switch 与 fan policy segmented control。
- Update 已收敛为 firmware check：artifact selector、compatibility verdict、dry-check progress 与 blocked/warning/success mock 状态。
- 全局日志使用 `@tanstack/react-virtual` + `simplebar-react` 渲染 1000 条 deterministic trace；事件按时间从上到下保序并将新事件追加到底部，滚动条只在 hover/滚动时显示。follow-tail 使用 Radix Switch，由用户显式切换，开启后以最新事件内容 key 驱动连续尾部定位，关闭后保留用户的历史滚动位置。
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
