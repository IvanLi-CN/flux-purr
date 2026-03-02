# Flux Purr 初始化（#n6csh）

## 状态

- Status: 已完成
- Created: 2026-03-02
- Last: 2026-03-02

## 背景 / 问题陈述

- 当前仓库在初始化前是空目录，缺少固件与 Web 的统一工程基线。
- 团队需要同时推进 Rust no-std 固件与 React Web 控制台，且要求 hooks、Storybook、shadcn 与发布策略从 Day-0 可用。
- 若不在初始化阶段完成规范对齐，后续会出现提交规范不一致、UI 契约缺失、发布规则漂移等问题。

## 目标 / 非目标

### Goals

- 建立单仓工程骨架：`firmware/` + `web/` + `docs/` + `.github/` + `scripts/`。
- 固件按 ESP32-S3 默认落地 no-std 基线，并预留 C3 切换位。
- Web 侧落地 React + Vite + Bun + Biome + shadcn/ui + Storybook + Playwright。
- 建立本地与 CI 一致的质量门禁：`pre-commit`、`commit-msg`、`pre-push`。
- 落地 label gate + release intent + 分域 tag（`fw/v*`、`web/v*`）脚本与工作流。
- 在仓库内安装 UI UX Pro Max，并补齐可执行使用指引。

### Non-goals

- 不创建 GitHub 远端仓库。
- 不执行 push、不创建 PR。
- 不执行 ESP32-C3 实际迁移（仅保留迁移说明与扩展点）。
- 不引入超出初始化所需的运行时业务功能。

## 范围（Scope）

### In scope

- Git 仓库初始化与 topic 分支创建。
- `docs/specs/` 规格目录与本规格文档。
- 固件基线 crate、S3 目标配置、构建说明。
- Web 基线工程、shadcn 组件与 Storybook stories。
- 仓库级 hooks、commitlint、检查脚本。
- CI/Release workflows 与版本/标签脚本。
- UI UX Pro Max 仓库内安装与使用指南。

### Out of scope

- 云端部署、设备 OTA、生产环境密钥配置。
- 板级外设映射与分区表细化。
- PR 合并、Release 发布执行。

## 需求（Requirements）

### MUST

- 存在目录：`firmware/`、`web/`、`docs/specs/`、`.github/workflows/`、`.codex/skills/ui-ux-pro-max/`。
- `lefthook` 覆盖 `pre-commit`、`commit-msg`、`pre-push`。
- commit message 使用英文 conventional commits，并可被 commitlint 阻断非法格式。
- Storybook 可构建，且核心页面/组件具备 stories。
- shadcn 组件同时可用于 App 与 Storybook。
- 固件 `fmt`、`clippy`、`build` 命令可执行且结果明确。
- Release 规则支持 PR label 驱动和 `workflow_dispatch` 手动兜底。

### SHOULD

- Web 与 hooks/CI 命令保持完全一致，减少“本地过、CI 挂”。
- 版本脚本可在无 GitHub 环境变量时本地演练。

### COULD

- 后续增加 Storybook 视觉基线截图与 Playwright 端到端用例。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 开发者在仓库根目录执行 `bun install` 后，能够安装并启用 git hooks。
- 开发者执行 `bash scripts/check-*.sh` 可复用与 CI 相同的检查路径。
- Web 通过 `bun run --cwd web storybook` 调试组件，通过 `build-storybook` 生成静态产物。
- 固件通过 `cargo` 命令完成 no-std crate 质量门禁；S3 目标方向在 README 与 CI/release 中保持一致。
- Release workflow 在 push main 场景下根据关联 PR labels 决策版本级别与发布通道，也支持手动触发。

### Edge cases / errors

- 无效 commit message 必须在 `commit-msg` 阶段失败并阻断提交。
- 缺失/冲突 PR labels 时，`label-gate.sh` 必须失败。
- 本地无 `HEAD` 场景（初始化早期）下，版本脚本 rc 逻辑需回退到 `local000`，避免脚本崩溃。
- E2E 默认可通过 `SKIP_E2E=1` 跳过，避免无测试样例时阻断主流程。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Device REST API | HTTP | external | New | ./contracts/http-apis.md | firmware + web | Web 控制台 | 草案阶段，后续需按硬件能力收敛 |
| Telemetry Stream | WebSocket | external | New | ./contracts/http-apis.md | firmware + web | Web 控制台 | 低频趋势流，前后端字段先对齐 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)

## 验收标准（Acceptance Criteria）

- Given 空目录初始化完成，When 检查项目结构，Then 必须看到 `firmware/`、`web/`、`docs/specs/`、`.github/workflows/`、`.codex/skills/ui-ux-pro-max/`。
- Given hooks 已安装，When 触发 `commit-msg` 且 message 非 conventional，Then 提交被拒绝并输出错误原因。
- Given Web 依赖已安装，When 执行 `bash scripts/check-web-build.sh` 与 `bash scripts/check-storybook-build.sh`，Then 两者均成功。
- Given 固件基线已创建，When 执行 `bash scripts/check-firmware-fmt.sh`、`bash scripts/check-firmware-clippy.sh`、`bash scripts/check-firmware-build.sh`，Then 全部成功。
- Given label 不完整，When 执行 `label-gate.sh`，Then gate 失败；Given label 完整，Then gate 通过。
- Given normal-flow 收口，When 输出交付状态，Then 明确 `PR-ready（local）` 且 `未 push / 未建 PR`。

## 实现前置条件（Definition of Ready / Preconditions）

- 流程类型已锁定为 `normal`。
- 技术路线已锁定：Firmware=S3 默认、Web=React+shadcn+Storybook。
- 发布策略已锁定：PR labels 自动决策 + workflow_dispatch 手动兜底。
- UI 工具链已锁定：仓库内安装 UI UX Pro Max。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `cargo test --manifest-path firmware/Cargo.toml`
- Integration tests: 初始化阶段不新增跨域集成测试；通过脚本验证端到端链路可执行性。
- E2E tests (if applicable): `SKIP_E2E=1 bash scripts/check-web-e2e.sh`（可选门）

### UI / Storybook (if applicable)

- Stories to add/update:
  - `web/src/stories/ConsoleLayout.stories.tsx`
  - `web/src/stories/DeviceStatusCard.stories.tsx`
  - `web/src/stories/WifiConfigForm.stories.tsx`
  - `web/src/stories/TelemetryTrendCard.stories.tsx`
- Visual regression baseline changes: 初始化阶段仅保证 `build-storybook` 成功，不引入快照基线系统。

### Quality checks

- `bun run hooks:install`
- `bash scripts/check-firmware-fmt.sh`
- `bash scripts/check-firmware-clippy.sh`
- `bash scripts/check-firmware-build.sh`
- `bash scripts/check-web-check.sh`
- `bash scripts/check-web-build.sh`
- `bash scripts/check-storybook-build.sh`

## 文档更新（Docs to Update）

- `README.md`: 仓库结构、快速开始与本地检查命令。
- `docs/interfaces/http-api.md`: 设备 HTTP/WS 草案接口。
- `docs/guides/ui-ux-pro-max.md`: 仓库内 UI UX Pro Max 使用流程与交付检查。
- `docs/specs/README.md`: 规格索引状态。

## 计划资产（Plan assets）

- None

## 资产晋升（Asset promotion）

- None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 建立仓库骨架与 spec gate（`docs/specs/**`）
- [x] M2: 落地 firmware S3 默认基线（no_std + async skeleton + toolchain/config）
- [x] M3: 落地 web 工程（React + shadcn + Storybook + stories）
- [x] M4: 落地 hooks/脚本与 CI/release workflows
- [x] M5: 本地验证完成并收口到 PR-ready（local）

## 方案概述（Approach, high-level）

- 以单仓分域方式组织 firmware/web，复用根目录脚本统一质量入口。
- 使用 shadcn primitives + Storybook 作为 UI 契约和可视验证基线。
- 使用 lefthook 将提交质量前置到本地，同时在 GitHub Actions 中复用同脚本。
- 通过 release-intent + compute-version 脚本把“标签驱动发布规则”显式化与可审计化。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：S3 -> C3 迁移时 toolchain、target 与外设驱动可能引入编译差异。
- 风险：当前接口文档为草案，后续硬件约束可能调整字段与采样频率。
- 开放问题：远端仓库命名与组织空间尚未确定。
- 假设：初始化阶段允许以 host 构建为本地默认验证路径，S3 Xtensa 由后续 CI/release 完整覆盖。

## 变更记录（Change log）

- 2026-03-02: 创建规格并锁定初始化范围、验收标准与里程碑。
- 2026-03-02: 完成 hooks、Storybook、shadcn、UI UX Pro Max、CI/release 及本地验证口径同步。

## 参考（References）

- [UI UX Pro Max](https://ui-ux-pro-max-skill.nextlevelbuilder.io)
- [UI UX Pro Max GitHub README](https://github.com/nextlevelbuilder/ui-ux-pro-max-skill)
- [shadcn Vite 安装](https://ui.shadcn.com/docs/installation/vite)
- [Storybook React + Vite](https://storybook.js.org/docs/get-started/frameworks/react-vite)
