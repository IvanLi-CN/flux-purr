# Flux Purr PR 标签发布与主分支保护决策记录（#r9k3m）

## Decision Log

- 采用 PR label release 模式：PR 标签是发布意图源，主线 snapshot 是发布执行源。
- 保留 Web 与 Firmware 的独立 release workflow 和 tag namespace，降低产物边界耦合。
- 使用 `.github/quality-gates.json` 记录 GitHub required checks 与主分支保护契约，避免只依赖 UI 状态。
- 远端 ruleset 不要求仓库 owner 创建的 PR 取得额外 reviewer approval；PR、签名提交和 required checks 仍保持强制。
