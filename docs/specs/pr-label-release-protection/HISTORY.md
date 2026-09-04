# Flux Purr PR 标签发布与主分支保护决策记录

## Legacy identity

- Former legacy ID: `r9k3m`.

## Lifecycle

- `active`: release labels and branch protection remain current repository policy.

## Decision Log

- 采用 PR label release 模式：PR 标签是发布意图源，VERSION-only 准备提交中的 trailers 是发布执行绑定。
- 产品发布收敛为单一 `vX.Y.Z` tag；Web、Firmware 与 host-tools 通过同一 Release 的 manifest 表达组件差异。
- 使用 `.github/quality-gates.json` 记录 GitHub required checks 与主分支保护契约，避免只依赖 UI 状态。
- 远端 ruleset 不要求仓库 owner 创建的 PR 取得额外 reviewer approval；PR、签名提交和 required checks 仍保持强制。
- `Label Gate` 保持只读验证；准备 workflow 在其成功后使用已有 PR-branch 写入能力创建 VERSION-only commit，因此不依赖 PR comment API。
- Release recovery reuses an immutable prepared main merge. 历史 snapshot-promotion record remains historical only.
