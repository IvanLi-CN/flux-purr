# Flux Purr PR label release protection（已退役）

> 本主题是历史记录，不再是活动发布契约。当前发布规则见 [Manual version release delivery](../manual-version-release-delivery/SPEC.md)。

## Historical status

Flux Purr 曾使用 `type:*` / `channel:*` PR labels、`Label Gate`、PR-local VERSION preparation 和 `Release completion` 绑定发布意图与版本边界。这些 workflow、脚本和 required checks 已删除；历史 tag、Release、资产和本主题只保留用于审计。

## Current replacement

- PR 仍然需要 review、required checks、signed commits 和 PR-only `main` protection，但不需要 release labels。
- 非纯文档变更默认通过 owner-confirmed manual `release_target` 形成实际产品 Release。
- owner 选择 `patch`、`minor`、`major` 或 `rc`；workflow 从 trusted stable baseline 独立解析 canonical version。
- 纯文档 allowlist 只包含 `docs/**`、`README*`、`CHANGELOG*`、Markdown、纯文本和文档图片资源；混合提交按产品变更处理。
- same-identity recovery 只复用指向当前 exact source SHA 的 release tag，不接受旧的 `commit_sha` recovery 入口。

## References

- [Manual release specification](../manual-version-release-delivery/SPEC.md)
- [Version source specification](../version-source/SPEC.md)
- [ADR 0025](../../adr/0025-manual-version-release-delivery.md)
