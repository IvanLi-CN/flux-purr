# Flux Purr PR 标签发布与主分支保护

## Related ADRs

- [0003-version-file-is-the-product-version-source](../../adr/0003-version-file-is-the-product-version-source.md)
- [0005-pr-local-version-preparation](../../adr/0005-pr-local-version-preparation.md)

## Supersession

This specification remains the contract for PR label validation, channel routing, and release failure context. Its former tag/package-based numeric version calculation is superseded by [the version-source specification](../version-source/SPEC.md) and ADR 0003. `Validate PR labels` remains a required check; `Release completion` is an additional mainline completion gate.

## 背景 / 问题陈述

Flux Purr 使用 PR label gate、PR-local version preparation、product release workflow 和 release 失败通知。标签通过和完整 PR CI 后，准备提交将 intent 固化为 trailers，避免 `push` 到 `main` 后重新读取可变 PR 标签。主分支保护所需的检查项由 repo-local 声明约束，避免 GitHub UI 配置与仓库配置漂移。

## 目标 / 非目标

### Goals

- PR 合入前必须具备确定的发布标签意图。
- `main` 上每个 pushed SHA 完成非抢占式 CI 验证后，才允许 product release workflow 执行。
- 发布 workflow 从准备提交 metadata 读取发布意图，而不是重新猜测可变 PR 标签。
- 仓库声明主分支保护、签名提交和 required checks 的期望状态。

### Non-goals

- 不改变 PR 标签作为发布意图的输入模型。
- 不把 release 失败 Telegram 告警替换为新的通知系统。
- 不在代码中绕过 GitHub 原生 branch protection；仓库 owner 创建的 PR 不要求额外 approval。

## 范围（Scope）

### In scope

- `type:*` 与 `channel:*` PR 标签规则。
- `CI PR`、`CI Main`、`Release Product` 的触发关系。
- VERSION-only 准备提交中的 release intent trailers。
- `.github/quality-gates.json` 作为分支保护声明。
- README 中的人类操作说明。

### Out of scope

- GitHub UI 中无法由当前工具直接写入的 ruleset 设置。
- 发布产物签名、SBOM、硬件烧录分发通道。
- 自动为旧提交补齐发布 intent。

## 需求（Requirements）

### MUST

- 每个 PR 必须恰好有一个 `type:*` 标签：`type:patch`、`type:minor`、`type:major`、`type:docs`、`type:skip`。
- 每个 PR 必须恰好有一个 `channel:*` 标签：`channel:stable`、`channel:rc`。
- 未知、缺失或重复的 release intent 标签必须让 label gate 失败。
- `type:docs` 与 `type:skip` 必须禁止 product release 发布。
- `type:patch|minor|major` 必须驱动单一 product release 发布；`type:patch + channel:stable` 自动发布，`type:minor`、`type:major` 与 `channel:rc` 需要显式 `exact` 后才能发布。
- 版本准备必须只在 `Validate PR labels` 与完整 PR CI 成功后运行；`Release Product` 只在 `CI Main` 成功后或显式 `recover` 时读取 prepared merge。
- Product host-tools release builds MUST use the workspace's version-controlled `Cargo.lock` with `--locked`.
- Ubuntu host-tools release jobs MUST install the system packages required by the locked workspace build, including `pkg-config` and `libudev-dev`, before running the release build.
- Every Ubuntu job that builds `flux-purr-devd` MUST use the shared Linux serial dependency action before the locked build.
- Manual `Release Product` recovery MUST accept an explicit prepared main merge SHA and recover its existing identity without recomputing release intent.
- A partial-run retry with an existing stable tag MUST verify that the tag points at the candidate and that any existing Release manifest matches the resolved source, version, channel, components, and asset hashes before reusing it.
- 主分支 required checks 必须包含 `Validate PR labels`、`Release completion`、`Firmware checks`、`DEVD checks`、`Web checks` 与 `Worktree bootstrap`。

### SHOULD

- Label Gate 与准备提交 metadata 只冻结发布意图；数字版本只从 root `VERSION` 读取。`type:patch + channel:stable` 的自动准备写入 `nextPatch(VERSION)`，其他发布由 `exact` 写入明确版本文本。
- Stable tag 使用 `vX.Y.Z`。
- RC tag 使用 `vX.Y.Z-rc.N`。
- 准备提交的 intent trailers 应保持幂等；后续 PR 标签变更必须使 `Release completion` 重新验证而不是改变已准备版本。

## 功能与行为规格（Functional / Behavior Spec）

### Core flows

- PR 打开、同步、重新打开、编辑或标签变更时，`Label Gate` 校验 release intent 标签。
- `Label Gate` 必须校验 release intent；准备工作流只在该 check 与完整 PR CI 成功后将 intent 写入 VERSION-only commit，避免 merge 后标签变更影响发布决策。
- PR CI 运行 firmware 和 web 检查，保持可抢占以节省无效分支运行时间。
- 合入 `main` 后，`CI Main` 以目标 SHA 隔离并结构验证 normal merge 是否保留准备提交；完整检查已在 PR source 执行一次。
- `Prepare product version` 以 main release train 为并发隔离键，避免不同 PR 同时修改同一版本基线。
- 准备 workflow 为该 PR source 创建唯一的 VERSION-only commit；trailers 只保存已冻结的 label intent，不保存或计算数字产品版本。
- `Release Product` 由 push 事件产生且成功的 `CI Main` 触发，只发布具有效准备提交的 main merge；它不从 label 解析数字版本。
- 手动 `recover` 必须显式提供已有 prepared main merge SHA，并复用该 commit。
- 手动 `exact` 必须显式提供一个已验证的 open PR 和严格高于当前 VERSION 的有效版本文本；其 RC/stable channel 必须与当前标签匹配。

### Edge cases / errors

- 准备 workflow 找不到 open in-repository PR、PR head 与已验证 checks 不一致、或 PR 已不基于 current main 时，必须失败而不是写入 VERSION。
- 准备提交 metadata 与当前 validated labels 不匹配，或其父源提交没有完整通过 `Validate PR labels`、Firmware、DEVD、Web 与 Worktree CI 时，`Release completion` 必须失败。
- `Release Product` 找不到以准备提交为第二父提交且 tree-equivalent 的 main merge 时，必须跳过而不是重新读取 PR 标签。
- `recover` 必须保持 main merge、VERSION、tag 与 source SHA 不变。
- 历史 snapshot 与未合入 main 的候选仅为可读审计记录，不能参与版本基线或数字版本计算。
- `type:docs` 或 `type:skip` 不创建准备提交且不发布产品资产。
- 已存在 release tag 时，发布 workflow 跳过 tag 创建但继续保持 rerun 幂等。
- release-completion gate 必须阻止未经准备的产品 PR 合入；不得以 queue、first-parent 补齐或压缩多个提交的方式决定版本。

## 主分支保护契约

- `main` 必须要求 PR 合入，不允许默认直接 push。
- `main` 必须禁止强推和删除。
- Commit 签名应为必需项。
- Required checks 必须使用 GitHub 显示的 job 名称，而不是本地别名。
- Review policy 优先用 GitHub 原生规则表达；仓库 owner 创建的 PR 不应因缺少 reviewer approval 被阻塞。

## 验收标准

- Given 一个含 `type:patch` + `channel:stable` 的 PR event，When 执行 label gate，Then 检查通过。
- Given 缺失、重复或未知 release intent 标签，When 执行 label gate，Then 检查失败。
- Given `type:docs` 或 `type:skip`，When release completion runs, Then it permits the no-product-release path without a VERSION commit。
- Given `Release Product` 被 `workflow_run` 触发，When 对应 `CI Main` 失败，Then release job 不发布。
- Given an enabled RC PR, When `Prepare product version` is dispatched with `operation=exact`, Then it writes the supplied `X.Y.Z-rc.N` text to that PR's VERSION-only preparation commit.
- Given `.github/quality-gates.json`，When 执行质量门禁校验，Then required checks 能映射到 repo-local workflow job。

## 非功能性验收 / 质量门槛

- Shell 测试覆盖标签 gate 与版本脚本的 stable/rc 输出。
- Python 脚本必须通过 `py_compile`。
- 本地验证覆盖 firmware fmt/clippy/build、web check/build 和 Storybook build。

## 文档更新

- README 必须说明 PR 标签、PR-local version preparation、手动 `recover` 和分支保护检查项。
- `.github/quality-gates.json` 必须作为 GitHub 远端保护设置的对齐依据。

## 风险与开放问题

- 当前 GitHub MCP 工具未暴露 branch protection/ruleset 写入接口时，只能提交 repo-local 声明并在 PR 中记录远端待对齐设置。
- GitHub Actions concurrency 不是严格 FIFO；准备 workflow 和 strict required checks 共同确保 stale PR 必须重新准备，不能复用旧版本提交。

## 假设

- `main` 是默认保护分支。
- 维护者希望 Web、Firmware 与 host-tools 对同一个 release intent 同步挂到单一 product Release。
