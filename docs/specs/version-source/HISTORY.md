# Flux Purr 单一产品版本源主题历史

> 这里记录主题局部生命周期、替换、兼容性与必要背景；完整 ADR 取舍保留在 `docs/adr/`。规范正文仍以 `./SPEC.md` 为准。

## Lifecycle / Compatibility

- 本主题是 Flux Purr 产品版本、开发 build identity、VERSION-only 准备提交和逐源提交发布顺序的当前契约。
- `docs/specs/pr-label-release-protection/` 中由 PR label、snapshot、channel 和 tag baseline 决定数字版本的部分已被本主题与 ADR 0003 取代；Label Gate、snapshot、channel routing、主分支保护和非版本化 PR policy 仍继续生效。
- 既有 Git tags、GitHub Releases、release manifests 与 Git notes 保持历史可读，但不再是新构建或新发布的版本输入。

## Replacements / Background

- `VERSION=0.23.0` 是当前 Version File migration baseline。历史 `v0.23.0` tag/release 指向旧的孤立发布边界，只保留为审计记录；首个完整新链路普通 patch 在同一 PR 上准备 `0.23.1`。exact intent 在准备提交中锁定所给文本，之后的每个产品 PR 都有独立版本边界。
- Candidate tag names are reserved before VERSION preparation. A foreign owner is a hard failure; recovery may reuse only the exact merged-main owner. This preserves the historical `v0.23.0` identity without retagging or rewriting it.
- 一产品 PR 一版本要求 VERSION-only 准备提交在正常 protected merge 前完成；该顺序替代了发布后直接写入 `main` 的设计。
- 自动 patch、受控 exact、main-merge recovery 与既有 `GITHUB_TOKEN` 权限边界由 ADR 0005 固化；Label Gate 继续只保存发布意图，不参与数字版本计算。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- [ADR 0003](../../adr/0003-version-file-is-the-product-version-source.md)
- [ADR 0004](../../adr/0004-release-commit-version-control.md)
- [ADR 0006](../../adr/0006-tag-reservation-and-legacy-release-reconciliation.md)
