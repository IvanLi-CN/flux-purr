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
- `verified` 必须同时满足段写入/ROM MD5 验证与 runtime identity/layout/install-status 验证；重连超时返回 `write_complete_unverified`。
- 失败报告只能由用户下载到本地，不自动上传，且不得包含配置原始字节、凭据或任意主机路径。
- 真实写入继续受 exact-port、lease、串口独占和 `FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1` 门禁约束。

### SHOULD

- 官方 catalog 默认 stable，RC 显式 opt-in，本地文件放在高级入口。
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
- 降级：默认阻止，只有高级确认 `allowDowngrade=true` 后可继续，并仍执行全部门禁。

### Edge cases / errors

- 端口消失或重新枚举：当前事务失败，绝不自动选择新端口。
- 安全响应未知或字段长度不符：`blocked`，不得尝试写入。
- 写入已完成但 runtime 未在时限内重连：`write_complete_unverified`，不得宣称成功。
- 空白或损坏 persistence：写入安全默认，`commissioningRequired=true`；旧有效 record 缺少该 TLV 时视为已完成。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `.fluxpurr-fw` | file format | external | New | `./contracts/file-formats.md` | bundle tool | Web/devd/release | strict ZIP |
| bundle manifest | JSON Schema | external | New | `./contracts/firmware-bundle.schema.json` | bundle tool | Web/devd | single schema |
| firmware bundle catalog/import | HTTP | external | New | `./contracts/http-apis.md` | devd | Web | content addressed |
| protected firmware operation | HTTP | external | New | `./contracts/http-apis.md` | devd | Web | approval-bound |
| install status | USB JSONL | external | New | `./contracts/device-install-status.md` | firmware | Web/devd | no secrets |

### 契约文档（按 Kind 拆分）

- [`contracts/file-formats.md`](./contracts/file-formats.md)
- [`contracts/http-apis.md`](./contracts/http-apis.md)
- [`contracts/device-install-status.md`](./contracts/device-install-status.md)
- [`contracts/firmware-bundle.schema.json`](./contracts/firmware-bundle.schema.json)
- [`contracts/migrations.json`](./contracts/migrations.json)

## 验收标准（Acceptance Criteria）

- Given 合法与恶意 ZIP fixtures，When 两个 validator 校验，Then 只接受三段完整、hash 正确、无路径风险且不超过 8 MiB 的 bundle。
- Given devd 可用或不可用，When 打开固件工作台，Then 默认选择 devd 或回退 Browser，并可在 preflight 前手动切换。
- Given 空片或外来固件 ESP32-S3FH4R2，When 选择 install/recovery，Then 不要求 Flux 身份，也不存在 PCB/heater 物理确认限制。
- Given update 目标温度无效或高于 40C，When preflight，Then 写入被阻止且 heater 已保持停止。
- Given未知 security 响应或安全功能启用，When preflight，Then 两条引擎均 fail closed。
- Given 写入和 ROM MD5 完成但 runtime 重连超时，When 结算，Then outcome 为 `write_complete_unverified`。

## 验收清单（Acceptance checklist）

- [x] 核心路径的长期行为已被明确描述。
- [x] 关键边界/错误场景已被覆盖。
- [x] 外部合同已拆分并链接。
- [x] 真机授权边界保持不变。

## 实现前置条件（Definition of Ready）

- [x] ESP32-S3FH4R2 layout、bundle 与状态机冻结。
- [x] update/recovery 的身份和 persistence 语义冻结。
- [x] transport 优先级、channel 和 downgrade 选择冻结。
- [ ] HIL 前由主人给出完整串口路径并明确允许全擦该目标 MCU。

## 非功能性验收 / 质量门槛（Quality Gates）

- `bun run check:firmware:fmt`
- `bun run check:firmware:clippy`
- `bun run check:firmware:build`
- `bun run check:devd`
- `bun run check:web`
- `bun run check:web:build`
- `bun run check:storybook`
- `bun run check:e2e`
- bundle、security、migration、fake SerialPort 和 fake espflash fixture suites。
- ui_demo desktop 与 `393x852` 视觉证据。

## 文档更新（Docs to Update）

- `docs/specs/README.md`
- `docs/interfaces/http-api.md`
- `docs/specs/35bta-eeprom-memory-config/SPEC.md`
- `docs/specs/m8r4q-real-control-plane-runtime/SPEC.md`
- `firmware/README.md`
- `web/README.md`

## 实现里程碑（Milestones / Delivery checklist）

- [ ] M1: bundle/layout/schema/fixtures 与 release artifact 合同
- [ ] M2: firmware identity、install status 与 commissioning persistence
- [ ] M3: devd bundle API 与 protected update/recovery transaction
- [ ] M4: Browser engine、统一状态机、UI/Storybook/E2E 与视觉证据
- [ ] M5: 非硬件门禁、授权 HIL、review 与 PR Step 5C 收敛

## 风险 / 开放问题 / 假设

- HIL 在主人提供精确端口与全擦授权前保持阻断；这不授权自动发现或切换端口。
- Web Serial 正式支持 HTTPS/localhost 下桌面 Chrome 与 Edge；其他浏览器只能使用可用 devd。
- stable 为默认源，RC opt-in，本地文件由用户信任但不豁免校验。

## 参考（References）

- `../m8r4q-real-control-plane-runtime/SPEC.md`
- `../35bta-eeprom-memory-config/SPEC.md`
- `../hhwq8-web-control-plane-demo/SPEC.md`
