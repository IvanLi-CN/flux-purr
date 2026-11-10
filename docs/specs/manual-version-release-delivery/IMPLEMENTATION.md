# Manual version release delivery implementation

## Current status

- Lifecycle: active.
- Release entry: `.github/workflows/release.yml` 的 manual `workflow_dispatch`。
- Policy implementation: `.github/scripts/manual_release.py`。
- Contract tests: `.github/scripts/test-manual-release.sh` 与 `.github/scripts/test-release-workflows.sh`。
- Product version source: root `VERSION`，由 `scripts/product-version.py` 供所有产品 consumer 读取。

## Implemented contract

- `release_target` 只有 `patch`、`minor`、`major`、`rc`；workflow 没有最终版本输入。
- workflow checkout `main` 并验证 `origin/main` exact SHA，读取 stable tag baseline，按变更路径执行文档-only skip 或产品 release。
- publish 在构建前预留 annotated tag；recover 只接受 tag 与当前 exact source SHA 匹配的 same identity。
- `rc` 使用下一个 stable baseline 的 `rc.N`，不推进 stable baseline；同 identity recovery 不增加 RC 序号。
- release manifest 固化 `releaseIntent`、`trustedBaseline`、`reservationId`、canonical version、tag、source SHA 与各 artifact digest。
- Release、EdgeOne production/demo 和 failure notification 继续分层；部署复用已发布 Web archive。
- CI PR/CI Main 只执行源代码质量检查，不再识别 prepared commit 或 release labels。
- `.github/quality-gates.json` 仅声明真实 PR/main workflow jobs；旧 Label Gate、Release preparation 和 Release completion 已从活动路径删除。

## Owner-side acceptance after merge

合并后需要 owner 在 GitHub Actions 中选择 intent，确认 run 的 source SHA、tag、Release manifest 和 artifact provenance 一致，再观察 EdgeOne 两个目标是否复用同一 archive。失败时只允许使用同一 tag@SHA 的 `operation=recover`。本地 contract tests 不替代这些远端写入与 provider 行为验证。
