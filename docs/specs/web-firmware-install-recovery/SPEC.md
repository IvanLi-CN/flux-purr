# Flux Purr Web 固件安装与恢复

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，关键演进原因见 `./HISTORY.md`。

## 背景 / 问题陈述

- Web 控制台现有 Web Serial 仅承载运行时 JSONL，更新页的烧录能力只来自本机 `flux-purr-devd`。
- DIY 场景中的目标可能是空片、来自其他设备的 ESP32-S3，或运行非 Flux Purr 固件，不能依赖运行时身份完成首次安装或恢复。
- Browser 与 devd 若各自定义包格式、安全检查和结果语义，会造成相同固件在两条路径上产生不同安全结论。

## 目标 / 非目标

### Goals

- 提供“更新现有 Flux Purr”和“安装或恢复”两个一等任务。
- 默认优先 devd，不可用时允许桌面 Chromium 在 HTTPS/localhost 上使用 Web Serial。
- 两条引擎共享唯一 `.fluxpurr-fw`、layout、migration、安全、状态机和结果合同。
- 允许安装到空片或非 Flux Purr 固件目标；恢复流程不询问、推断或限制 PCB/加热器连接状态。
- 对更新流程保全 `flux_cfg`，并使 EEPROM 始终独立于 MCU internal-flash 擦写。

### Non-goals

- 不做 A/B、OTA rollback、断点续烧、静默重试或自动选择重新枚举的端口。
- Web 不接受裸 BIN、ELF、手工地址、任意 ESP32 板型或 LAN 烧录。
- 不引入固件签名、证书或“官方签名”措辞。
- 不让 `mcu-agentd` 参与实现、测试或验收。

## 范围（Scope）

### In scope

- `firmware/` build identity、layout、install status 与 commissioning persistence。
- `tools/flux-purr-devd/` bundle parser/packager、bundle API 和受保护烧录事务。
- `web/` bundle catalog/import、Browser Web Serial 烧录引擎、统一状态机和任务优先 UI。
- release workflow 与 product manifest 的 `.fluxpurr-fw` 产物。

### Out of scope

- 旧 devd-only ELF/raw CLI 开发者接口的移除。
- 未经主人精确端口授权的任何串口、复位、擦除或烧录操作。
- GitHub Release 正式发布与 PR merge。

## 需求（Requirements）

### MUST

- Bundle 必须符合 `contracts/firmware-bundle.schema.json`，ZIP 解压前后均不得超过 8 MiB，且严格只含一个 manifest 和 bootloader、partition-table、factory-app 三段镜像。
- 目标固定为 ESP32-S3FH4R2、4 MiB Flash、2 MiB PSRAM、DIO/40 MHz；段地址固定为 `0x0`、`0x8000`、`0x10000`，边界由 `firmware/flash-layout.json` 定义。
- 每段声明实际长度、SHA-256 和 ESP ROM MD5；未知字段、路径穿越、重复、缺段、重叠、越界或 hash 不一致均 fail closed。
- Browser 与 devd 只接受 registry 中声明的 migration ID；update 的当前 partition-table SHA-256 必须精确匹配 migration source。
- update 仅适用于可验证 Flux Purr runtime；烧录前必须停热并取得有效温度 `<=40°C`，保全、迁移并逐字验证 `flux_cfg`。
- install/recovery 允许无 Flux 身份并全擦 MCU internal Flash；不得提出、推断或执行任何 PCB/加热器物理连接确认或限制。
- Secure Boot、Flash Encryption、Secure Download Mode、未知安全响应、非 ESP32-S3 或非 4 MiB Flash 一律阻止。
- 写入中断后不得续传或静默重试；重新执行必须从完整 preflight 开始。
- devd update preflight 必须在进入 ROM 前从目标运行时重新读取 identity 与 status；不得使用发现缓存代替当前版本、停热和温度事实。独立 ROM preflight 一旦成功建立 ROM 连接，无论探测结论通过或阻止，都必须在返回前复位目标回到 runtime。
- devd 的单次烧录事务只能在开始时进入 ROM 一次，并在全部验证完成后复位到 runtime 一次；中间的擦除、段写入、ROM MD5 与 `flux_cfg` 保全命令必须保持 `no-reset`，不得为每个子命令复位目标。
- `verified` 必须同时满足段写入/ROM MD5 验证与 runtime identity/layout/install-status 验证；重连超时返回 `write_complete_unverified`。
- 失败报告只能由用户下载到本地，不自动上传，且不得包含配置原始字节、凭据或任意主机路径。
- Browser 预检必须在本地记录用户点击、`requestPort()` 发起、端口选择或拒绝、运行时连接、ROM 连接和终态的有序追踪；该追踪必须写入本地诊断报告，且不得包含配置原始字节、凭据或任意主机路径。
- 真实写入继续受 exact-port、lease、串口独占和 `FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1` 门禁约束。
- Browser 只能读取当前 Web origin 的 `firmware/releases-manifest.json` 与其中声明的精确 `firmware/releases/**.fluxpurr-fw` 路径；不得访问 GitHub API、GitHub download URL、任意目录路径或任意代理 URL。

### SHOULD

- 固件包选择必须是单一入口，打开组件库对话框后在“发布版本”和“本地文件”之间切换；发布版本采用受视口约束的左右两栏，左栏仅承载 RC opt-in、当前选择与说明，右栏通过共享 `ScrollArea` 呈现独立可滚动的版本列表。发布对话框高度为 `min(36rem, 100dvh - 4rem)`，版本列表必须占满 tabs 与公共操作区之间的全部可用高度，列表不得按稳定版或候选版分组，必须按 `publishedAt` 倒序展示；非稳定版本必须显示 `RC` chip，默认选中最新 stable。本地文件由用户信任但不豁免校验，且不把发布渠道本身伪装成可写入固件。
- 正式 release 构建必须在服务器侧分页读取 GitHub Releases REST API 的所有非 draft 版本，以 `Accept: application/octet-stream` 下载并严格验证有效 `.fluxpurr-fw`，再写入 Web 静态包中的同源目录与 `firmware/releases-manifest.json`。当前 release bundle 必须在 GitHub Release 创建前一并写入该目录。
- 本地 Vite 开发服务必须接管固定 `/firmware/**`，将已打包 release、服务器端 GitHub Releases 结果和本地产物合并为一份同源目录；本地产物由 `bun run build:firmware:web` 直接写入默认 `firmware/target/flux-purr-web-artifacts/`，Vite 必须监听该目录、仅在进程内缓存并响应原字节，绝不拷贝到 `web/public` 或其他目录。优先级依次为已打包 release、GitHub release、本地产物。以 `sourceSha + buildId` 相同的 artifact 冲突时，后者覆盖前者。GitHub 刷新失败时仍返回已打包和本地产物，不把失败传给 Browser。
- devd 可用时自动选中，但在操作开始前允许切换 Browser；开始后 transport 冻结。
- Browser manual BOOT/reset fallback 必须给出可操作状态，而非自动猜测端口变化。

### COULD

- 后续增加签名版本的 bundle schema，但不得改变 v1 未签名 bundle 的语义。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

统一状态机：`artifact -> transport -> rom_reset -> chip_flash_security -> layout_config -> preflight -> erase? -> write_segments -> rom_md5 -> reset -> runtime_reconnect -> runtime_verify`。

- Update：要求已识别 runtime；停热与温度门禁通过后保存 `flux_cfg`，按同布局或声明 migration 写入三段，恢复并逐字验证配置，最后验证目标 identity/install status。
- Install/recovery：直接从 ROM security/chip/flash preflight 开始，全擦 internal Flash 后写三段；EEPROM 不属于擦除范围；首次启动进入 setup-required，heater 保持 locked。
- 传输选择：devd health 可用时默认 devd；否则符合条件的 Chromium 提供 Browser；LAN 永不提供 flash capability。
- Browser ROM/stub 顺序：Browser 先通过 ROM bootloader 完成芯片、Flash package 和 `GET_SECURITY_INFO` 安全预检；只有通过后才上传 esptool stub 读取布局或写入。写入必须消费同一连接的成功预检状态，不能重新在 stub 状态探测安全信息或绕过 ROM 预检。
- 降级：默认阻止，只有高级确认 `allowDowngrade=true` 后可继续，并仍执行全部门禁。
- 工作区导航：设备控制与固件维护是同级根工作区。设备态顶部固定为设备选择器、热板、PD、工作区切换；设备选择器只包含设备与连接方式。固件态不显示设备选择器、设备页签或设备运行日志，而显示仅包含当前烧录任务的固件事务日志；未选择控制设备时同样不显示总览、设置或校准页签。

### Edge cases / errors

- 端口消失或重新枚举：当前事务失败，绝不自动选择新端口。
- Browser `GET_SECURITY_INFO`：接受恰好 20 字节的完整安全记录，或不少于 24 字节的完整 ROM/扩展响应；后者只解释协议定义的前 20 字节并忽略 ROM transport trailer 与后续扩展。少于 20 字节、21–23 字节截断响应、未知安全位或任一安全功能启用时：`blocked`，不得尝试写入。
- 写入已完成但 runtime 未在时限内重连：`write_complete_unverified`，不得宣称成功。
- 空白或损坏 persistence：写入安全默认，`commissioningRequired=true`；旧有效 record 缺少该 TLV 时视为已完成。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `.fluxpurr-fw` | file format | external | New | `./contracts/file-formats.md` | bundle tool | Web/devd/release | strict ZIP |
| bundle manifest | JSON Schema | external | New | `./contracts/firmware-bundle.schema.json` | bundle tool | Web/devd | single schema |
| release catalog | JSON Schema | external | New | `./contracts/firmware-release-catalog.schema.json` | release builder | Web/Vite | same-origin only |
| firmware bundle catalog/import | HTTP | external | New | `./contracts/http-apis.md` | devd | Web | content addressed |
| protected firmware operation | HTTP | external | New | `./contracts/http-apis.md` | devd | Web | approval-bound |
| install status | USB JSONL | external | New | `./contracts/device-install-status.md` | firmware | Web/devd | no secrets |

### 契约文档（按 Kind 拆分）

- [`contracts/file-formats.md`](./contracts/file-formats.md)
- [`contracts/http-apis.md`](./contracts/http-apis.md)
- [`contracts/device-install-status.md`](./contracts/device-install-status.md)
- [`contracts/firmware-bundle.schema.json`](./contracts/firmware-bundle.schema.json)
- [`contracts/firmware-release-catalog.schema.json`](./contracts/firmware-release-catalog.schema.json)
- [`contracts/migrations.json`](./contracts/migrations.json)

## 验收标准（Acceptance Criteria）

- Given 合法与恶意 ZIP fixtures，When 两个 validator 校验，Then 只接受三段完整、hash 正确、无路径风险且不超过 8 MiB 的 bundle。
- Given devd 可用或不可用，When 打开固件工作台，Then 默认选择 devd 或回退 Browser，并可在 preflight 前手动切换。
- Given 空片或外来固件 ESP32-S3FH4R2，When 选择 install/recovery，Then 不要求 Flux 身份，也不存在 PCB/heater 物理确认限制。
- Given update 目标温度无效或高于 40C，When preflight，Then 写入被阻止且 heater 已保持停止。
- Given未知 security 响应或安全功能启用，When preflight，Then 两条引擎均 fail closed。
- Given Browser 收到 20 字节安全记录或不少于 24 字节的完整 ROM/扩展响应，When preflight，Then 从前 20 字节一致解析安全位；Given 21–23 字节截断响应，Then `blocked`。
- Given 写入和 ROM MD5 完成但 runtime 重连超时，When 结算，Then outcome 为 `write_complete_unverified`。
- Given Browser 打开发布版本选择器，When 读取目录并选择版本，Then 只请求同源 `releases-manifest.json` 和清单中的精确 bundle 路径；Given Vite 开发模式中 GitHub 暂时不可用，Then 仍列出已打包和本地产物。

## 验收清单（Acceptance checklist）

- [x] 核心路径的长期行为已被明确描述。
- [x] 关键边界/错误场景已被覆盖。
- [x] 外部合同已拆分并链接。
- [x] 真机授权边界保持不变。

## 实现前置条件（Definition of Ready）

- [x] ESP32-S3FH4R2 layout、bundle 与状态机冻结。
- [x] update/recovery 的身份和 persistence 语义冻结。
- [x] transport 优先级、channel 和 downgrade 选择冻结。
- [x] devd HIL 已在主人给出的单一精确串口与明确全擦授权下执行；授权不延伸到任何重新枚举的端口。

## 非功能性验收 / 质量门槛（Quality Gates）

- `bun run check:firmware:fmt`
- `bun run check:firmware:clippy`
- `bun run check:firmware:build`
- `bun run check:devd`
- `bun run check:web`
- `bun run check:web:build`
- `bun run check:e2e`
- bundle、security、migration、fake SerialPort 和 fake espflash fixture suites。
- ui_demo desktop 视觉证据。

## 文档更新（Docs to Update）

- `docs/specs/README.md`
- `docs/interfaces/http-api.md`
- `docs/specs/35bta-eeprom-memory-config/SPEC.md`
- `docs/specs/m8r4q-real-control-plane-runtime/SPEC.md`
- `firmware/README.md`
- `web/README.md`

## Visual Evidence

桌面 `ui_demo=firmware-workspace` 使用确定性 mock 数据，不依赖 devd、浏览器串口或硬件设备。

非 Demo 入口支持 `?workspace=firmware` 直达固件维护工作区；本地构建包由同源目录提供，页面不会请求或操作浏览器串口直到用户明确运行预检。

PR: none
![设备控制工作区](assets/firmware-workspace-device.png)

PR: none
![固件维护发布版本选择器](assets/firmware-workspace-firmware-release-picker.png)

PR: none
![固件维护统一滚动版本列表](assets/firmware-workspace-firmware-scroll-area.png)

PR: none
![固件维护本地文件选择器](assets/firmware-workspace-firmware-local-picker.png)

PR: none
![同源发布目录选择器](assets/firmware-release-catalog-same-origin-desktop.png)

PR: none
![固件维护预检等待态](assets/firmware-workspace-browser-preflight-ui-demo.png)

PR: none
![固件事务日志跟随最新记录](assets/firmware-transaction-log-scroll.png)

PR: none
![固件事务日志历史滚动与返回最新入口](assets/firmware-transaction-log-history.png)

## 实现里程碑（Milestones / Delivery checklist）

- [ ] M1: bundle/layout/schema/fixtures 与 release artifact 合同
- [ ] M2: firmware identity、install status 与 commissioning persistence
- [ ] M3: devd bundle API 与 protected update/recovery transaction
- [ ] M4: Browser engine、统一状态机、UI/E2E 与视觉证据
- [ ] M5: 非硬件门禁、授权 HIL、review 与 PR Step 5C 收敛

## 风险 / 开放问题 / 假设

- 每次 HIL 都要求主人提供精确端口与全擦授权；该授权不允许自动发现、重新枚举或切换端口。devd 与 Browser HIL 分别验收。
- Web Serial 正式支持 HTTPS/localhost 下桌面 Chrome 与 Edge；其他浏览器只能使用可用 devd。
- stable 为默认源，RC opt-in；发布版本在 Browser 中始终通过同源静态目录读取，本地文件由用户信任但不豁免校验。

## 参考（References）

- `../m8r4q-real-control-plane-runtime/SPEC.md`
- `../35bta-eeprom-memory-config/SPEC.md`
- `../hhwq8-web-control-plane-demo/SPEC.md`
