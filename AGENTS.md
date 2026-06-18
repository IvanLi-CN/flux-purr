# AGENTS 指南

本文件适用于 `flux-purr` 仓库内的所有 Agent 操作。除非主人明确覆盖，必须优先遵守本文件；若与更高层系统/开发者规则冲突，按更高层规则执行。

## 项目概览

- 本仓是嵌入式固件 + Web 控制台 mono-repo。
- `firmware/`：Rust `no_std` ESP32-S3 固件，主二进制为 `flux-purr`。
- `web/`：React + Vite + Storybook 控制台。
- `tools/flux-purr-devd/`：本机 native daemon，用于 USB/serial discovery、lease、monitor、Wi-Fi bridge、固件 artifact 检查和受保护烧录。
- `docs/specs/`：长期规格与验收契约。
- `docs/solutions/`：可复用工程经验。
- `docs/hardware/`：硬件基线、引脚、电源链路与网表。

## 开发者规范真相源与 Skill 路由

- 人类可读的本地项目开发者规范真相源是 `docs/guides/flux-purr-developer-policy.md`。
- 开发者角色 Agent 先读取 `skills/flux-purr-developer-policy/SKILL.md`；它负责 repo 级开发者约束和 skill 分流。
- 需要 repo 内 `devd`、CLI、Web/native bridge、release、校准、烧录、mock smoke、HIL 或真机验证时，再读取 `skills/flux-purr-developer-operations/SKILL.md`；它是仓库内 developer operations/HIL 专项 skill，对表 installed or released user surface，不承载完整 repo 级行为规范。
- owner-facing 的已发布产品操作才使用 `skills/flux-purr-user-operations/SKILL.md`；不要把 released/user 流程误用为本仓开发流程。
- 当任务需要 IsolaPurr 作为外部 HUB、USB-C 供电通路或 bench source 时，再读取 `$isolapurr-developer-operations`；它只约束外部设备一侧，不替代 Flux Purr 项目流程。
- Flux Purr 自身 daemon 和 CLI 必须优先从本仓 `tools/flux-purr-devd` 运行，例如 `cargo run --manifest-path tools/flux-purr-devd/Cargo.toml --bin flux-purr-devd -- serve ...` 和 `cargo run --manifest-path tools/flux-purr-devd/Cargo.toml --bin flux-purr -- ...`。

## 不可绕过的 Repo 级硬门禁

### MCU 设备端口授权

- 任何烧录、监视、复位、串口读写、`mcu-agentd selector set`、`espflash`、`esptool` 或等价 MCU 操作，只允许使用主人明确授权的设备端口。
- 当前若主人只授权 `/dev/cu.usbmodem21221401`，则只能使用该端口；不得把任何重新枚举出的 `/dev/cu.*` / `/dev/tty.*` 当作同一目标继续操作。
- 未经主人明确授权，严禁把目标设备从一个 `/dev/cu.*` / `/dev/tty.*` 端口切换到另一个端口，即使系统只枚举出一个候选端口、端口看起来像同一块板、MAC 地址相似或重新枚举后路径变化。
- 若授权端口消失、变号、无法打开、被占用、下载模式下重新枚举为新路径，必须立即停止 MCU 操作并向主人报告当前证据；不得自动修改 selector、不得自动选择新端口、不得继续烧录。
- `mcu-agentd selector set` 视为更换目标设备授权边界的高风险操作。除非主人明确给出新端口或明确授权切换，否则禁止执行。
- 只读排查新枚举端口是否可能是同一物理设备时，必须先声明只读范围；排查结果只能报告给主人，不得升级为烧录、复位、selector 修改或目标切换。
- `mcu-agentd` 操作必须使用当前项目的目标 `esp32s3_frontpanel`；不得在其它仓库或其它 MCU 目标上执行烧录、复位、监视或 selector 修改。

### HUB 与外部设备边界

- Isolapurr HUB 控制页面只可用于给已授权目标端口对应的物理链路断电/上电。
- 严禁把 HUB 控制设备、HUB 固件仓库、HUB 的 MCU selector 或其它 ESP32 设备当作当前项目目标 MCU。
- 操作 HUB 电源前必须确认目标是 USB-C 口电源控制，而不是对 HUB 固件本身进行烧录或监视。
- HUB 重新上电后如果 ESP32 端口路径变化，仍必须遵守“端口授权硬纪律”，不能自动切换到新路径。

### 源码开发与真实写入边界

- 涉及本机设备能力的源码开发流程应优先通过 `tools/flux-purr-devd` 和既有 Web/native bridge，不要临时绕过安全检查。
- 不得为了让日志“看起来正常”而屏蔽传感器故障、按键故障或保护逻辑；必须保留安全失败路径。
- `mcu-agentd` 不是 CLI/`devd` HIL 的默认 acceptance path；除非主人明确改计划，否则不得用它替代 `flux-purr` through `devd` 的验收路径。
- 真实烧录默认禁用；除非主人明确授权并满足端口硬纪律，不得设置或依赖 `FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1` 执行真实写入。

### 文档与交付

- `docs/specs/**` 是长期 topic-level specification；规格、实现状态与演进原因应保持同步。
- `docs/solutions/**` 用于沉淀跨任务可复用经验。
- 编写文档时不要加入“本次修改”“新增说明”等修订痕迹；文档应描述当前事实。
- 若实现改变硬件行为、用户可见行为、API 契约或安全边界，必须同步相关 spec/project docs。
- 使用 Conventional Commits，commit message 使用英文并带 `--signoff`。
- 未经主人明确要求，不执行 `git push`。
- 不得擅自改变 remote、upstream、pushurl、credential helper 或协议配置。
- 工作区可能已有主人改动；不得回滚或覆盖非本次修改。
