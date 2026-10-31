# Flux Purr 热控 Bench Web Demo

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，关键演进原因见 `./HISTORY.md`。

## Related ADRs

- [ADR 0003: Transport-scoped WiFi provisioning](../../adr/0003-transport-scoped-wifi-provisioning.md)
- [ADR 0019: Separate live console and Demo runtime](../../adr/0019-separate-live-console-and-demo-runtime.md)

## Context and Scope

- `docs/solutions/device-control/web-native-wifi-bridge-console.md` 已沉淀 Web、native USB daemon、USB CDC、WiFi provisioning、firmware flashing 与 monitoring 的长期控制面方案。
- 本轮 Web demo 不应呈现为管理后台，也不应让用户面对完整 fleet / operations / dashboard 信息墙。
- 当前需要的是一个轻量固定控制台：Dashboard 显示热控运行态，Settings 以 tabs 分开 preset / fan policy / WiFi 配置，Update 做固件 dry-check，桌面端全局日志可与当前页面同时显示。

## 目标 / 非目标

### Goals

- 提供独立的 `control-plane-demo` Web console，由 TanStack Router 驱动设备作用域页面与可分享深链。
- 把产品形态收敛成固定高度 bench console：顶部辅助状态栏、Dashboard / Settings / Update 三个界面、桌面端常驻全局日志。
- 使用 deterministic mock fixtures 展示 devd、serial、mock 三类设备，不接真实硬件。
- 保留工业拟物视觉：浅色 chassis、物理按钮、内凹数据槽、LED、暗色全局 trace 面。
- 提供可收起的 Demo Inspector，以 typed URL 复现确定性 scene、lease/network/artifact 故障与模拟动作；公开构建始终为 mock-only 完整控制台。
- 保留现有 `device-console` 与 `frontpanel-preview` 代码和 Storybook stories，不做破坏性重构。
- Storybook 提供 default、degraded、gallery、mobile review 与交互 smoke coverage。

### Non-goals

- 不做管理后台、fleet dashboard、artifact catalog 管理页或全量控制平面配置面。
- Mock-only Demo build 不启用 native USB daemon、WiFi HTTP 或真实 firmware flashing；live Web console 复用已有 Browser Web Serial、devd 和 LAN adapters。
- 不引入后端服务、认证、凭据持久化或真实 artifact catalog。
- 不使用 hash routing，也不把 transport target ID、设备别名、网络地址或凭据写入 URL。
- 不替换 `160×50` 前面板 preview 或既有前面板 specs。
- 不建立生产级视觉回归系统；本 spec 只要求 mock UI 视觉证据。

## 范围（Scope）

### In scope

- `web/src/features/control-plane-demo/**`
- `web/src/stories/ControlPlaneDemo.stories.tsx`
- `web/src/App.tsx`
- `web/src/routes/**` 与 TanStack Router app shell
- `web/src/index.css` 中的工业风 token 与轻工具组件样式
- 本 spec 目录与视觉证据资产
- `docs/solutions/device-control/web-native-wifi-bridge-console.md` 的相关 spec 关联
- `web/README.md` 的 Web demo 说明

### Out of scope

- `firmware/**`
- native daemon 工具链
- 真实设备 API 契约变更

## Requirements

### Stable requirement identifiers

- `REQ-WEB-DEMO-001`：正式构建 MUST 固定 live，公开 Demo 构建 MUST 固定 mock；旧模式参数不得改变运行时边界。
- `REQ-WEB-DEMO-002`：Demo 初始条件 MUST 由 typed URL 表达，产品操作、Inspector action、日志和任务 MUST 共享内存会话。
- `REQ-WEB-DEMO-003`：当前设备、故障和能力覆盖 MUST 只作用于路由 identity；保护状态 MUST 优先于已有操作结果。
- `REQ-WEB-DEMO-004`：Reset MUST 原子恢复完整默认基线并丢弃迟到异步结果，校准离开取消时 URL 与模型 MUST 保持不变。
- `REQ-WEB-DEMO-005`：WiFi 显式 null 回执 MUST 保持清空字段；分享链接 MUST 只恢复初始条件且复制失败不得提示成功。
- `REQ-WEB-DEMO-006`：Inspector MUST 保持宽屏 dock、桌面浮层、移动 drawer/bubble、键盘可达和触控目标约束。
- `REQ-WEB-DEMO-007`：Demo 初始化失败 MUST 停留在安全错误页面，正式构建 MUST 不请求 Demo 的 mock 或旧模式 API。

### MUST

- Demo 必须在无硬件、无 daemon、无网络服务的情况下完整渲染。
- Demo 必须始终保持一屏轻工具心智模型；桌面端默认不依赖页面滚动。
- Demo 必须提供 `Dashboard`、`Settings`、`Update` 三个界面，不得把所有能力堆成一个长页面。
- 生产 Web App 必须以 TanStack Router 作为页面、设备与校准 workspace tab 的唯一导航真相源。
- 规范路径必须为 `/devices/:deviceId/overview`、`/devices/:deviceId/settings/presets`、`/devices/:deviceId/settings/fan`、`/devices/:deviceId/settings/wifi`、`/devices/:deviceId/update`、`/devices/:deviceId/calibration/heater-curve`、`/devices/:deviceId/calibration/rtd-adc`、`/devices/:deviceId/calibration/vin-adc` 与 `/devices/new`；旧 `/devices/:deviceId/settings` 必须 replace 到 presets。
- `:deviceId` 必须使用稳定物理 `identityId`；transport target ID 只能用于本地连接候选解析。
- `/` 必须 replace 到当前 variant 最近设备的 overview；没有有效记录时 replace 到 `/devices/new`。`/devices`、`/devices/:deviceId` 与 `/devices/:deviceId/calibration` 必须 replace 到对应规范页。
- 正式构建必须固定 live，公开 Demo 构建必须固定 mock；URL、localStorage 和旧模式参数不得改变运行时边界。正式构建收到 `demo=true` 或 `uiDemo` 时显示入口停用页，公开 Demo 清理这些旧参数。
- `demoScene`、`demoLease`、`demoNetwork`、`demoArtifact`、`demoWifiCapability`、`demoLogs` 与 `demoTelemetry` 必须由 router 作为 typed search 参数验证和规范化；它们只表达 Demo 的场景初始条件，不表达产品操作回放。
- 结构无效的路径必须显示 404；结构有效但设备未知或不可连接时必须保留原 URL，并提供重试、选择设备与添加连接动作。
- 桌面端必须提供全局日志面板，并能与 `Dashboard` / `Settings` / `Update` 当前内容同时显示。
- 桌面端全局日志不得退化成窄侧栏；在宽桌面上应以可读 trace console 呈现，保留足够行宽阅读 message。
- Demo 不得出现侧边管理导航、后台式指标墙或多层 fleet 管理结构。
- Demo 必须支持选择至少三个 mock device target，并显示当前 transport、firmware、build、thermal runtime 摘要。
- Dashboard 必须把热板运行状态作为第一层信息，当前温度必须是首要视觉焦点。
- Dashboard 必须显示目标温度、heater 输出、PD 合约电压、风扇/主动降温状态和最常用的热控辅助动作。
- Dashboard 的目标温度必须能直接实时调整，不得依赖提交/应用按钮。
- Settings 必须使用链接语义的 workspace tabs 分开展示 live summary/preset temperatures/preset enable、fan policy 和 transport-scoped WiFi settings。
- Settings 中 preset 温度调整必须在 debounce 后自动保存；不得提供额外提交按钮。
- WiFi tab 必须根据选中 Device 的 capability 与 transport 隐藏、只读或读写；Web Serial 与 active devd USB lease 可写，WiFi/LAN 与 devd LAN bridge 只读并显示 Alert 原因。
- WiFi mock 与 live 表单在 10 秒等待后必须将“取消”映射为 Device `wifi_config(op=cancel)`；只有 device-confirmed `idle` 才能显示取消成功，不能用停止本地等待代替设备操作。
- Update 必须展示 artifact 选择、compatibility verdict、dry-check progress 与结果摘要，但不得呈现为完整 artifact 管理后台。
- 全局日志必须支持至少 1000 条 mock trace，通过虚拟列表渲染；滚动条仅在 hover/滚动时显示。
- Storybook 必须提供 default、degraded、gallery、mobile review 与交互 smoke story。
- 所有可点击控件必须具备 hover/focus/active 视觉反馈，移动端触控目标高度不低于 48px。
- 一级页面 tabs 与校准 workspace tabs 必须使用链接语义、支持键盘操作，并正确表达 `aria-current`。
- 校准运行期间，站内 Link、程序跳转、设备切换、variant/search 变化与浏览器 Back/Forward 必须共用同一离开保护；页面关闭或地址栏导航必须启用原生 `beforeunload`。
- 生产静态构建必须包含 EdgeOne history fallback，将 `/devices` history pathname rewrite 到 `/index.html`，且不得拦截静态资源。
- `build:demo` 必须生成独立静态 artifact，并在运行时强制 mock fixture、关闭 devd/Web Serial/direct LAN；根路径必须 replace 到 `/devices/fp-lab-01/overview`，清理旧模式参数与本地 live preference。Demo 初始化失败必须停留在安全错误页面，不得回退 live。

### SHOULD

- Accent 色只用于主动作、状态 LED、危险或当前选择，不作为大面积装饰色。
- 深色技术屏和日志区应轻量嵌入，不主导整个页面。
- 长字符串应换行或截断到容器内，不允许撑破布局。

### COULD

- 后续可在同一轻工具 shell 上接入真实 contract fixtures 或 generated API schema。
- 后续若要做完整控制台，应另建真实控制台 spec，不在本轻 demo 中扩张。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- `Device`
  - 展示 `devd`、`serial`、`mock` 三类 target。
  - 设备切换是低频操作，只能放在顶部辅助状态栏，不得成为独立主模块或占用主工作区空间。
  - 选择 target 后，顶部状态栏和当前动作上下文切换到对应设备。
  - 设备切换保留当前页面或校准 tab 的 route suffix；添加连接成功后进入新设备 overview。
  - 深链恢复按最近成功 transport、当前 active/healthy、LAN、唯一已授权且匹配的 Web Serial、devd/bridge、匹配 demo mock 的顺序串行尝试。
  - 自动恢复不得调用 `requestPort()`；没有唯一预授权 Web Serial 候选时只能提供显式用户动作。
- `Routing`
  - 生产 app shell 持续挂载 `ControlPlaneDemo`，leaf route 只提供类型化 `ConsoleRouteState`，切换 tabs 不得重建 transport/runtime 状态。
  - Storybook 未提供 production navigation adapter 时继续使用 `initialView` 与组件本地 state。
  - 正式构建中的 `uiDemo=true` 仅保留为旧入口停用判定；公开 Demo 忽略并清理该参数。普通正式根路径按 live 最近设备规则跳转，公开 Demo 根路径固定到 `fp-lab-01` overview。
  - 公开 Demo build 把根路径固定到 `fp-lab-01` overview，Inspector 的 scene、目标故障、能力和数据 query 使用 client-side replace；面板布局状态不写入 URL。
  - 分享链接只恢复场景、当前设备、故障及能力/数据开关的初始条件；复制必须等待路由提交，产品操作结果、临时日志、任务、计时器与凭据不进入链接。
  - 设备偏好使用版本化本地结构，只保存 variant 对应 identity 和 identity 对应 transport kind；损坏值必须忽略。
- `Dashboard`
  - 当前温度是唯一主视觉，不得被连接、WiFi 或日志抢占层级。
  - 展示 target temp、heater output、PD contract、fan policy / active cooling 等热控摘要。
  - 目标温度通过同屏 stepper 直接调整，变更立即反映到 mock runtime 和日志。
  - 展示 Hold heater 等热控辅助动作。
  - 连接/transport 只保留在顶部辅助状态栏，不得作为 Dashboard 主指标。
- `Settings`
  - 展示 live target 与当前 preset slot summary，用分隔线把状态展示与可编辑设置分开。
  - 展示 10 个 preset slots，slot 可选择，disabled preset 必须明显降权。
  - 选中 preset 后可以调整 preset 温度，并使用 `Switch` 启用/禁用该 preset。
  - preset 温度变更在 debounce 后自动保存并写入 mock log；没有 Apply / Use as target 按钮。
  - fan policy 用 segmented control 表达，变更立即反映到 mock runtime 和日志。
- `Update`
  - 首屏展示所选 artifact 的 compatibility verdict。
  - 用户可以选择兼容、warning 或 blocked mock artifact；blocked artifact 必须禁用 dry-check。
  - dry-check 只模拟本地校验和进度，不执行真实写入；完成后按钮进入可再次运行状态。
- `Global log`
  - 桌面端与当前界面同时显示 bounded trace rows。
  - Dashboard 使用完整可滚动日志面板；Settings / Update 使用紧凑 trace summary，降低干扰。
  - 日志列表必须使用虚拟列表承载 1000 条 mock trace；follow-tail 是显式 opt-in，不得强制滚动到底部。
  - 主操作按钮只改变 mock state 和视觉 affordance，不连接真实 host 能力。

### Edge cases / errors

- Offline target 不得显示虚假的 RSSI 或运行时成功状态。
- Degraded target 或 blocked artifact 必须以当前动作面呈现，而不是隐藏在日志里。
- 长字符串必须换行或截断到容器内，不允许撑破布局。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `ControlPlaneScenario` | TypeScript type | internal | New | None | web | demo app-shell / Storybook | Mock scenario model only |
| `DeviceTarget` | TypeScript type | internal | New | None | web | demo app-shell / Storybook | Mock target model only |
| `ControlPlaneDemo` | React component | internal | New | None | web | `App.tsx` / Storybook | Pure web light tool |
| `ConsoleRouteState` | TypeScript union | internal | New | 本文 | web | router app shell / `ControlPlaneDemo` | Device identity, page and optional calibration tab |
| `ConsoleNavigationAdapter` | TypeScript interface | internal | New | 本文 | web | router app shell / `ControlPlaneDemo` | Controlled production navigation with Storybook fallback |
| `DemoInspectorState` | TypeScript type | internal | New | 本文 | web | public demo app shell / Inspector | Deterministic fixture and fault state only |
| Device route schema | URL contract | public Web surface | New | 本文 | web | users / browser history / EdgeOne | Stable identity path and typed demo Inspector search |

### 契约文档（按 Kind 拆分）

- `None`

## 验收标准（Acceptance Criteria）

- Given 本地只安装 Web 依赖，When 执行 `bun run --cwd web build`，Then demo app 能成功构建。
- Given Storybook，When 打开 `Pages/ControlPlaneDemo`，Then 可以看到 default、degraded、gallery、mobile review 与 interaction smoke stories。
- Given 375px mobile viewport，When 查看 demo，Then 顶部状态栏、页签、当前页面与日志不出现不可读重叠。
- Given 1440px desktop viewport，When 查看 demo，Then Dashboard 与全局日志同屏显示，且页面本身不依赖大段滚动。
- Given 在顶部状态栏选择 `Field Kit`，When 查看当前界面，Then 页面显示该 target 的 serial transport、warning severity 与 firmware 摘要。
- Given 在 Dashboard 调整目标温度，When 点击 `+`，Then live target、mock runtime 与日志同步变化，不需要提交按钮。
- Given 点击 `Settings`，When 查看当前界面，Then 可以看到 preset slots、preset temperature editor、preset enable switch 与 fan policy control。
- Given 修改 preset 温度，When debounce 时间结束，Then preset 自动保存并显示 autosaved 状态。
- Given 禁用某个 preset，When 查看 preset grid，Then 对应 slot 变为 disabled 状态，且仍可重新启用。
- Given 点击 `Update`，When 切换 firmware artifact，Then 可以看到 compatibility verdict、dry-check progress 与 blocked/warning 状态。
- Given 点击 `Degrade mock`，When 查看当前页面，Then 能看到 degraded runtime 或 blocked artifact state。
- Given 任意规范设备深链，When 直接打开、刷新或使用 Back/Forward，Then 设备、页面、校准 tab 与 active state 均由 URL 恢复。
- Given 缺少 pathname 或打开索引路径，When router 解析位置，Then 正式构建按 live 最近设备规则处理，公开 Demo replace 到 `fp-lab-01` overview；旧 `demo` / `uiDemo` 输入不会改变运行时。
- Given 一个结构有效但未知的 `identityId`，When 自动恢复无法匹配连接，Then pathname 保持不变并显示可操作恢复态，而不是跳到其他设备。
- Given 校准模式正在运行，When 用户切换页面、tab、设备或浏览器历史，Then 离开确认先退出模式，只有成功后才继续导航；取消或失败保持原路由。
- Given 自动恢复 Web Serial，When 没有唯一已授权且匹配的 port，Then 页面不调用 `requestPort()` 并要求用户显式选择。
- Given EdgeOne 部署产物，When 刷新任意规范 history 深链，Then `edgeone.json` 将请求 rewrite 到 `/index.html` 并由 router 恢复页面。
- Given public Demo build，When 打开 `/?demo=false&uiDemo=true`，Then 始终进入完整 mock-only console，不出现 LAN pairing 页面且不会启动 devd、Web Serial 或 direct LAN。
- Given formal build，When 打开 `/?demo=true` 或任意带 `uiDemo` 的旧入口，Then 显示入口停用页，不挂载 live 控制台。
- Given Demo Inspector，When 切换 scene、lease/network/artifact、能力/数据覆盖或模拟动作，Then console 与全局 trace 同步更新；场景初始条件可刷新恢复，临时操作不回放；选择 Calibration active 时同一校准离开保护生效。
- Given Demo 中选定设备，When 注入 lease/network fault 或 thermal warning，Then 故障只作用于当前 URL identity，保护态覆盖已有操作值，fan recovery 不能解除有效安全锁。
- Given Demo Reset，When 已完成温度、preset、fan、WiFi 或任务操作，Then 原子恢复 normal 与默认开关，保留当前设备和页面，并清理所有设备临时结果、日志、表单、任务、计时器及迟到回调。
- Given WiFi clear receipt，When 设备明确返回成功且字段为 null，Then SSID、RSSI、密码长度与失败码保持空值，不被旧快照回填。
- Given WiFi 配置等待达到 10 秒，When operator 选择取消，Then mock fixture 与 live Web Serial 都等待 `cancel` receipt 的 `idle` 结果；等待、确认成功与取消失败分别有可辨识状态，且取消不会清空保存的 SSID 或密码掩码。

## Verification

### VER-WEB-DEMO-001

- Method: 正式与 Demo 独立构建、formal/public E2E 隔离报告以及无真实 API 请求断言。
- covers: `REQ-WEB-DEMO-001`, `REQ-WEB-DEMO-007`
- Pass condition: 正式旧入口停在停用页，Demo 根路径与设备深链固定 mock，初始化失败不回退 live，静态资源不被 history fallback 吞掉。

### VER-WEB-DEMO-002

- Method: DemoSession 与 Inspector state unit tests，加上 public Demo Playwright 场景、设备、故障、能力/数据、分享和刷新用例。
- covers: `REQ-WEB-DEMO-002`, `REQ-WEB-DEMO-003`, `REQ-WEB-DEMO-005`
- Pass condition: 产品和 Inspector 操作回显于同一会话，URL 只恢复初始条件，未知设备保持 identity，WiFi null 与剪贴板拒绝均无错误回填或假成功。

### VER-WEB-DEMO-003

- Method: reducer generation/reset unit tests、路由 Back/Forward E2E 和校准离开保护交互测试。
- covers: `REQ-WEB-DEMO-004`
- Pass condition: Reset 完整恢复默认基线，迟到回调不能恢复旧结果，取消确认保持原路由和模型，确认后正确清理。

### VER-WEB-DEMO-004

- Method: DemoInspector Storybook interaction stories 与固定视口 `ui_demo` 截图证据。
- covers: `REQ-WEB-DEMO-006`
- Pass condition: 数据/权限/复制失败/重置状态可交互验证，1700×1000、1440×1000、1024×900 和 375×812 无横向溢出，Inspector dock/floating/drawer/bubble 均可达。

## 验收清单（Acceptance checklist）

- 核心路径的长期行为已被明确描述。
- 关键边界/错误场景已被覆盖。
- 涉及的接口/契约已写清楚或明确为 `None`。
- 相关验收条件已经可以用于实现与 review 对齐。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bun run --cwd web check`
- `bun run --cwd web typecheck`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test:storybook`
- `bun run --cwd web test:unit`
- `bun run check:e2e`

### UI / Storybook

- Stories to add/update:
  - `web/src/stories/ControlPlaneDemo.stories.tsx`
- Docs pages / state galleries to add/update:
  - `Pages/ControlPlaneDemo / Docs / Gallery`
- `play` / interaction coverage to add/update:
  - `Pages/ControlPlaneDemo / InteractionSmoke`

### Quality checks

- 视觉证据通过 mock-only public `ui_demo` 覆盖 desktop、mobile、Inspector 与未知设备恢复态。
- owner-facing 图片必须先通过 immutable snapshot 回传。

## Visual Evidence

- 证据来源：独立 Vite mock-only Demo 构建，deterministic fixtures。
- 绑定说明：当前 PR 图来自 Chrome 中的正常控制台，固定为 `fp-lab-01` normal 场景；未知 identity 恢复态只作为历史辅助证据。
- 布局检查：1440×1000 保持 1280px 固定工作区且 Inspector 为非侵入 bubble；仅 1700px+ 在保留 1280px 工作区与 24px gutter 后 dock Inspector。375×812 无横向溢出或越界元素，Inspector bubble 与恢复操作触摸高度不小于 48px。
- Public Demo target fixtures display English names: `Bench Fixture A`, `Field Kit`, and `Offline Mock Device`.

### Current Public Demo Console with Inspector (1746×1136)


![Current Public Demo Console with Inspector](./assets/public-demo-inspector-canonical-console.jpg)

### Current Public Demo Inspector drawer (375×812)


![Current Public Demo Inspector drawer](./assets/public-demo-inspector-current-mobile.jpg)

### Calibration leave guard dialog


![Calibration leave guard dialog](./assets/calibration-leave-guard-dialog.jpg)

## Historical Visual Reference

### Unknown device recovery (auxiliary)


![Public Demo unknown device recovery](./assets/public-demo-inspector-unknown-recovery.jpg)

### Public Demo target fixture names


![Public demo Inspector English target fixtures](./assets/public-demo-inspector-english-targets.png)

### Public Demo Inspector desktop 1440×1000


![Public demo Inspector desktop](./assets/public-demo-inspector-desktop.png)

### Public Demo Inspector wide dock 1700×1000


![Public demo Inspector wide dock](./assets/public-demo-inspector-wide-docked.png)

### Public Demo Inspector tablet 1024×900


![Public demo Inspector tablet](./assets/public-demo-inspector-tablet.png)

### Public Demo Inspector mobile 375×812


![Public demo Inspector mobile](./assets/public-demo-inspector-mobile.png)

### Routed UI Demo desktop 1440×1000


![Routed UI demo desktop](./assets/routing-ui-demo-desktop.png)

### Routed UI Demo mobile 375×812


![Routed UI demo mobile](./assets/routing-ui-demo-mobile.png)

### Unknown device recovery 1440×1000


![Unknown device recovery](./assets/routing-unknown-device.png)

### Dashboard desktop 1440px


![Control plane demo desktop](./assets/control-plane-demo-desktop.png)

### Settings desktop 1440px


![Control plane demo settings](./assets/control-plane-demo-settings.png)

### Update desktop 1440px


![Control plane demo update](./assets/control-plane-demo-update.png)

### Mobile 375px


![Control plane demo mobile](./assets/control-plane-demo-mobile.png)

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：工业拟物阴影在低端设备上可能比扁平工具更重；当前 demo 仍保持 CSS-only，无额外 JS 动画计算。
- 风险：Storybook viewport addon 未配置时，mobile story 通过固定容器宽度表达移动审查面。
- 假设：`#27` solution 是长期控制平面架构真相源，但本 spec 只承接其中的轻量 Web 工具展示面。
- 假设：本 demo 只表达 Web UX 与 mock contract，不代表真实 transport 已交付。

## 参考（References）

- `docs/solutions/device-control/web-native-wifi-bridge-console.md`
- `web/README.md`
