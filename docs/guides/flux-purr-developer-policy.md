# Flux Purr Developer Policy

本文件是 Flux Purr 仓库内“本地项目开发者行为规范”的唯一人类真相源。

`AGENTS.md` 只保留 repo 级路由与不可绕过的硬门禁。`skills/flux-purr-developer-policy` 是开发者角色 agent 的 repo 级总约束入口。`skills/flux-purr-developer-operations` 保持为仓库内 developer operations/HIL 专项 skill，不再承担完整 repo 级开发者规范。

## 角色与工作面

- Repo developer：在当前 checkout 内开发 `firmware/`、`web/`、`tools/flux-purr-devd/`、`docs/` 与 release automation。
- Developer agent：与 repo developer 同 scope，但必须先读取 `skills/flux-purr-developer-policy/SKILL.md`，再按任务继续读取专项 skill。
- Product user/operator：通过 released `flux-purr`、`flux-purr-devd` 或 browser Web Serial 操作已发布产品；这是 `skills/flux-purr-user-operations` 的范围，不是 source-tree 开发流程。

## 默认开发路径

- 源码开发默认从当前仓库 checkout 运行工具，不依赖全局安装的 `flux-purr` 或 `flux-purr-devd`。
- Flux Purr 自身 daemon 和 CLI 必须优先从 `tools/flux-purr-devd` 运行。
- `devd`、CLI、Web/native bridge、release、校准、烧录、mock smoke、真机验证与 HIL 路径，统一由 `skills/flux-purr-developer-operations` 约束。
- owner-facing 的已发布产品操作，统一由 `skills/flux-purr-user-operations` 约束；不要把 released/user 路径误用为本仓开发流程。
- 当 Flux Purr 使用 IsolaPurr 作为外部 HUB、USB-C 供电通路或 bench source 时，只把 `$isolapurr-developer-operations` 用于外部设备一侧，不得替代 Flux Purr 项目流程。

## 子系统基线

- 固件主入口是 `firmware/src/bin/flux_purr.rs`；共享域逻辑在 `firmware/src/lib.rs` 与子模块中。
- 默认 MCU 方向是 ESP32-S3，当前硬件基线是 `ESP32-S3FH4R2`。
- 对引脚、ADC、按键映射、电源控制以及 heater/fan safety 的改动，先查 `docs/hardware/` 与相关 `docs/specs/`。
- Web UI 变更必须同时考虑 `web/` 运行时、Storybook 入口和必要测试。
- 当前运行时与发布基线以 `README.md` 和相关 `docs/specs/**` 为准；需要改变这些事实时，先更新文档再以实现对齐。

## Skill 路由

- 角色是开发者，且任务发生在本仓 source tree 内时，先读取 `skills/flux-purr-developer-policy/SKILL.md`。
- 任务涉及 `devd`、CLI、Web/native bridge、release、calibration、artifact verify、dry-run flash、real flash、mock HIL 或 real HIL 时，再读取 `skills/flux-purr-developer-operations/SKILL.md`。
- 任务是 owner-facing released-tool 操作、用户级硬件记忆、released manifest 升级判断或 browser Web Serial 正式用户路径时，改走 `skills/flux-purr-user-operations/SKILL.md`。
- 需要控制 IsolaPurr 外部 HUB/source 时，同时读取 `$isolapurr-developer-operations`，但只在外部设备边界内操作。

## 验证与工具链要求

- 常用本地验证入口：
  - `bun run check:firmware:fmt`
  - `bun run check:firmware:clippy`
  - `bun run check:firmware:build`
  - `bun run check:devd`
  - `bun run check:web`
  - `bun run check:web:build`
  - `bun run check:storybook`
  - `bun run check:e2e`
- ESP32-S3 release 构建基线：

```bash
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --release
```

- 非硬件验证先于任何真机/HIL 操作完成。
- `devd` 启动、CLI 调用和 Web live development 要显式使用当前 repo checkout、显式 bind/port 和显式环境变量，不依赖默认端口或全局二进制。
- `scripts/devd-hardware-smoke.py --device-id mock-fp-lab-01 --allow-mock-device` 只证明 localhost HTTP contract；不得把 mock smoke 报告成硬件验证。
- Web live development 在需要 leased `devd` 端口时，必须显式设置 `VITE_FLUX_PURR_DEVD_URL` 与 `VITE_FLUX_PURR_ENABLE_DEVD=1`。

## 文档与交付要求

- 任何改变硬件行为、用户可见行为、API 契约、安全边界或 release policy 的实现，都必须同步相关 `docs/specs/**`、`docs/solutions/**` 与项目文档。
- `docs/specs/**` 承载长期 topic-level specification；`docs/solutions/**` 承载跨任务可复用经验。
- 项目文档描述当前事实，不写修订痕迹，不写“本次修改”“新增说明”“版本记录”。
- 提交使用英文 Conventional Commits，并带 `--signoff`。
- 除主人明确要求外，不执行 `git push`，也不擅自改 remote、upstream、pushurl、credential helper 或协议配置。
- 工作区可能已有主人的改动；不得回滚、覆盖或顺手整理与当前任务无关的修改。

## 不可绕过的安全与授权边界

- 任何烧录、复位、串口读写、`mcu-agentd selector set`、`espflash`、`esptool` 或等价 MCU 操作，都只能使用主人明确授权的设备端口。
- 授权端口消失、变号、重新枚举或被占用时，必须停止并报告证据；不得自动切换端口。
- `mcu-agentd` 操作只允许针对当前项目目标 `esp32s3_frontpanel`；不得把其它 MCU 目标或其它仓库设备当作当前目标。
- `mcu-agentd` 不是 CLI/`devd` HIL 的默认 acceptance path；除非主人明确改计划，否则不要用它替代 `flux-purr` through `devd` 的验收路径。
- IsolaPurr HUB 只可作为外部电源/链路控制边界，不能当作当前项目目标 MCU、host tool 或 release surface。
- 真实烧录默认禁用；只有主人明确授权且满足精确端口授权边界时，才允许走 real flash。
- 固件与热控改动不得为了让日志“更正常”而屏蔽传感器故障、按键故障或保护逻辑；安全失败路径必须保留。
