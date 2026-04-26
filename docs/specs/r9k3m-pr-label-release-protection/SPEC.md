# Flux Purr PR 标签发布与主分支保护（#r9k3m）

## 状态

- Status: 已完成
- Owner: Flux Purr maintainers
- Scope: GitHub Actions、PR 标签策略、发布快照、主分支质量门禁声明

## 背景 / 问题陈述

Flux Purr 已具备 PR label gate、Web/Firmware 发布 workflow 和 release 失败通知，但发布触发仍由 `push` 到 `main` 后即时读取 PR 标签决定，缺少主线验证后的不可变发布意图快照。主分支保护所需的检查项也没有 repo-local 声明，GitHub UI 配置与仓库配置容易漂移。

## 目标 / 非目标

### Goals

- PR 合入前必须具备确定的发布标签意图。
- `main` 上每个 pushed SHA 完成非抢占式 CI 验证后，才允许 Web/Firmware 发布 workflow 执行。
- 发布 workflow 从不可变快照读取发布意图，而不是重新猜测可变 PR 标签。
- 仓库声明主分支保护、签名提交和 required checks 的期望状态。

### Non-goals

- 不改变 Web 与 Firmware 当前产物类型。
- 不把 release 失败 Telegram 告警替换为新的通知系统。
- 不在代码中绕过 GitHub 原生 branch protection / review policy。

## 范围（Scope）

### In scope

- `type:*` 与 `channel:*` PR 标签规则。
- `CI PR`、`CI Main`、`Release Web`、`Release Firmware` 的触发关系。
- `refs/notes/release-snapshots` 中的 release snapshot。
- `.github/quality-gates.json` 作为分支保护声明。
- README 中的人类操作说明。

### Out of scope

- GitHub UI 中无法由当前工具直接写入的 ruleset 设置。
- 发布产物签名、SBOM、硬件烧录分发通道。
- 自动为旧提交批量补齐 release snapshot。

## 需求（Requirements）

### MUST

- 每个 PR 必须恰好有一个 `type:*` 标签：`type:patch`、`type:minor`、`type:major`、`type:docs`、`type:skip`。
- 每个 PR 必须恰好有一个 `channel:*` 标签：`channel:stable`、`channel:rc`。
- 未知、缺失或重复的 release intent 标签必须让 label gate 失败。
- `type:docs` 与 `type:skip` 必须禁止 Web 和 Firmware 发布。
- `type:patch|minor|major` 必须同时驱动 Web 和 Firmware 发布。
- 发布 workflow 必须只在 `CI Main` 成功后或显式手动 backfill 时读取 release snapshot。
- 主分支 required checks 必须至少包含 `Validate PR labels`、`Firmware checks`、`Web checks`。

### SHOULD

- 版本计算优先基于已有同类稳定 tag 的最大 semver，再应用 PR 标签中的 bump level。
- Stable tag 使用 `web/vX.Y.Z` 与 `fw/vX.Y.Z`。
- RC tag 使用 `web/vX.Y.Z-rc.<sha7>` 与 `fw/vX.Y.Z-rc.<sha7>`。
- Release snapshot 写入应保持幂等，已有 snapshot 不应被后续 PR 标签变更覆盖。

## 功能与行为规格（Functional / Behavior Spec）

### Core flows

- PR 打开、同步、重新打开、编辑或标签变更时，`Label Gate` 校验 release intent 标签。
- `Label Gate` 必须把已校验的 release intent 绑定到 PR head SHA 并写入冻结 marker，避免 merge 后标签变更影响发布决策。
- PR CI 运行 firmware 和 web 检查，保持可抢占以节省无效分支运行时间。
- 合入 `main` 后，`CI Main` 以目标 SHA 隔离并以非抢占方式重新运行 firmware 和 web 检查。
- Release workflow 以目标 commit 作为并发隔离键，避免不同 `main` commit 的 pending release run 互相替换。
- `CI Main` 通过后，`Release Snapshot` 根据合入 commit 关联的唯一 PR 读取对应 PR head SHA 的冻结 marker，并把发布意图写入 git notes。
- `Release Web` 与 `Release Firmware` 由 push 事件产生且成功的 `CI Main` 触发，读取对应 commit 的 snapshot 后决定发布或跳过。
- 手动 backfill 必须显式提供 `main` 上的 commit SHA，并读取已有 snapshot。

### Edge cases / errors

- 合入 commit 找不到唯一关联 PR 时，snapshot 生成失败。
- 找不到与 PR head SHA 匹配的冻结 release intent marker 时，snapshot 生成失败。
- 首次合入本机制时，若目标 commit 的父提交尚不具备冻结 marker gate，允许一次性从 PR 当前标签生成 rollout snapshot；后续 commit 必须存在冻结 marker。
- Snapshot 缺失时，release workflow 失败而不是重新读取 PR 标签。
- `type:docs` 或 `type:skip` 的 snapshot 导出 `release_enabled=false`。
- 已存在 release tag 时，发布 workflow 跳过 tag 创建但继续保持 rerun 幂等。
- 连续合入多个 stable release PR 时，后续 snapshot 必须把已冻结但尚未发布的前序 stable snapshot 纳入版本基线。
- 后续 main commit 先完成 CI 时，snapshot 生成必须按 first-parent 补齐缺失的近邻祖先 snapshot，再计算目标 commit 的版本。
- 多个 main CI job 同时写入 release snapshot notes 时，必须 fetch 最新 notes、重放本 job 的缺失 note 并重试 push。

## 主分支保护契约

- `main` 必须要求 PR 合入，不允许默认直接 push。
- `main` 必须禁止强推和删除。
- Commit 签名应为必需项。
- Required checks 必须使用 GitHub 显示的 job 名称，而不是本地别名。
- Review policy 优先用 GitHub 原生规则表达；如果工具无法直接写入远端规则，仓库声明即为待对齐的 source of truth。

## 验收标准

- Given 一个含 `type:patch` + `channel:stable` 的 PR event，When 执行 label gate，Then 检查通过。
- Given 缺失、重复或未知 release intent 标签，When 执行 label gate，Then 检查失败。
- Given `type:docs` 或 `type:skip`，When 导出 release snapshot，Then `release_enabled=false`。
- Given `Release Web` 或 `Release Firmware` 被 `workflow_run` 触发，When 对应 `CI Main` 失败，Then release job 不发布。
- Given `.github/quality-gates.json`，When 执行质量门禁校验，Then required checks 能映射到 repo-local workflow job。

## 非功能性验收 / 质量门槛

- Shell 测试覆盖标签 gate 与版本脚本的 stable/rc 输出。
- Python 脚本必须通过 `py_compile`。
- 本地验证覆盖 firmware fmt/clippy/build、web check/build 和 Storybook build。

## 文档更新

- README 必须说明 PR 标签、发布触发、手动 backfill 和分支保护检查项。
- `.github/quality-gates.json` 必须作为 GitHub 远端保护设置的对齐依据。

## 实现里程碑

- [x] Workflow 拆分为 PR CI、Main CI、Release-after-main-CI。
- [x] Release snapshot 与版本计算脚本落地。
- [x] 质量门禁声明与校验落地。
- [x] 标签、版本、CI、文档验证完成。

## 风险与开放问题

- 当前 GitHub MCP 工具未暴露 branch protection/ruleset 写入接口时，只能提交 repo-local 声明并在 PR 中记录远端待对齐设置。
- GitHub Actions concurrency 不是严格 FIFO；CI Main 与 Release workflow 使用目标 SHA 隔离并配合 snapshot reconciliation 降低 rapid merge 丢发布风险。

## 假设

- `main` 是默认保护分支。
- 维护者希望 Web 与 Firmware 对同一个 release intent 同步发布或同步跳过。
