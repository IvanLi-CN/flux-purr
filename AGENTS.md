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

## 默认开发入口

- 当前开发者与 same-identity agents 在本仓执行 source checkout、devd/CLI、firmware/Web 集成、HIL、release automation 或仓库级硬件验证时，默认先读并遵循 `skills/flux-purr-developer-operations/SKILL.md`。
- 该 skill 是本仓 developer flow 的默认可发现入口；只有当任务明确是普通终端用户操作时，才切到 `skills/flux-purr-user-operations/SKILL.md`。
- 上述默认入口不替代本文件；端口授权、HUB 边界、文档与 Git 纪律仍以本 `AGENTS.md` 为准。

## IsolaPurr 边界

- 涉及 IsolaPurr 侧 source checkout、`isolapurr-devd` / `isolapurr`、HUB bench、电源链路、发布资产、校准或 HIL 的工作，不属于 Flux Purr developer flow。
- 这类任务默认切换到 `$isolapurr-developer-operations`，并按该 skill 的 checkout gate、hardware safety 与 release maintenance 规则执行。
- 在 Flux Purr 仓库内引用 IsolaPurr，只应用于外部 bench / HUB / telemetry 边界；不得把 IsolaPurr 仓库、host tools、selector 或发布流程当作本仓默认开发入口。

## 常用验证命令

- 固件格式：`bun run check:firmware:fmt`
- 固件 Clippy：`bun run check:firmware:clippy`
- 固件构建：`bun run check:firmware:build`
- devd：`bun run check:devd`
- Web 检查：`bun run check:web`
- Web 构建：`bun run check:web:build`
- Storybook：`bun run check:storybook`
- E2E：`bun run check:e2e`

ESP32-S3 release 构建基线：

```bash
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --release
```

## MCU 设备端口授权硬纪律

- 任何烧录、监视、复位、串口读写、`mcu-agentd selector set`、`espflash`、`esptool` 或等价 MCU 操作，只允许使用主人明确授权的设备端口。
- 当前若主人只授权 `/dev/cu.usbmodem21221401`，则只能使用该端口；不得把任何重新枚举出的 `/dev/cu.*` / `/dev/tty.*` 当作同一目标继续操作。
- 未经主人明确授权，严禁把目标设备从一个 `/dev/cu.*` / `/dev/tty.*` 端口切换到另一个端口，即使系统只枚举出一个候选端口、端口看起来像同一块板、MAC 地址相似或重新枚举后路径变化。
- 若授权端口消失、变号、无法打开、被占用、下载模式下重新枚举为新路径，必须立即停止 MCU 操作并向主人报告当前证据；不得自动修改 selector、不得自动选择新端口、不得继续烧录。
- `mcu-agentd selector set` 视为更换目标设备授权边界的高风险操作。除非主人明确给出新端口或明确授权切换，否则禁止执行。
- 只读排查新枚举端口是否可能是同一物理设备时，必须先声明只读范围；排查结果只能报告给主人，不得升级为烧录、复位、selector 修改或目标切换。
- `mcu-agentd` 操作必须使用当前项目的目标 `esp32s3_frontpanel`；不得在其它仓库或其它 MCU 目标上执行烧录、复位、监视或 selector 修改。

## HUB 与外部设备边界

- Isolapurr HUB 控制页面只可用于给已授权目标端口对应的物理链路断电/上电。
- 严禁把 HUB 控制设备、HUB 固件仓库、HUB 的 MCU selector 或其它 ESP32 设备当作当前项目目标 MCU。
- 操作 HUB 电源前必须确认目标是 USB-C 口电源控制，而不是对 HUB 固件本身进行烧录或监视。
- HUB 重新上电后如果 ESP32 端口路径变化，仍必须遵守“端口授权硬纪律”，不能自动切换到新路径。

## 固件开发纪律

- 固件主入口是 `firmware/src/bin/flux_purr.rs`；共享域逻辑在 `firmware/src/lib.rs` 与子模块中。
- 默认目标是 ESP32-S3，硬件基线为 `ESP32-S3FH4R2`。
- 当前运行时基线包括：
  - CH224Q 默认请求 `20 V`；
  - PPS 覆盖 20 V 时优先使用 `PPS/AVS + MOS static switching`；
  - 否则回退到 fixed-PD `GPIO47` PWM backend；
  - Dashboard center double toggles active-cooling；
  - Fan display line 使用 `OFF / AUTO / RUN`。
- 对引脚、ADC、按键映射、电源控制、heater/fan safety 的改动必须先查 `docs/hardware/` 与相关 `docs/specs/`。
- 不得为了让日志“看起来正常”而屏蔽传感器故障、按键故障或保护逻辑；必须保留安全失败路径。

## Web 与 devd 纪律

- Web UI 变更必须同时考虑 `web/` 运行时、Storybook 入口和必要测试。
- 涉及本机设备能力的浏览器流程应优先通过 `tools/flux-purr-devd` 和既有 Web/native bridge，不要临时绕过安全检查。
- 真实烧录默认禁用；除非主人明确授权并满足端口硬纪律，不得设置或依赖 `FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1` 执行真实写入。

## 文档与规格

- `docs/specs/**` 是长期 topic-level specification；规格、实现状态与演进原因应保持同步。
- `docs/solutions/**` 用于沉淀跨任务可复用经验。
- 编写文档时不要加入“本次修改”“新增说明”等修订痕迹；文档应描述当前事实。
- 若实现改变硬件行为、用户可见行为、API 契约或安全边界，必须同步相关 spec/project docs。

## Git 与交付

- 使用 Conventional Commits，commit message 使用英文并带 `--signoff`。
- 未经主人明确要求，不执行 `git push`。
- 不得擅自改变 remote、upstream、pushurl、credential helper 或协议配置。
- 工作区可能已有主人改动；不得回滚或覆盖非本次修改。
