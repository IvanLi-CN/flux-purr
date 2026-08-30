# Flux Purr 单一产品版本源主题历史

> 这里记录主题局部生命周期、替换、兼容性与必要背景；完整 ADR 取舍保留在 `docs/adr/`。规范正文仍以 `./SPEC.md` 为准。

## Lifecycle / Compatibility

- 本主题是 Flux Purr 产品版本、开发 build identity、Release Commit 和逐源提交发布顺序的当前契约。
- `docs/specs/r9k3m-pr-label-release-protection/` 中由 PR label、snapshot、channel 和 tag baseline 决定数字版本的部分已被本主题与 ADR 0003 取代；Label Gate、snapshot、channel routing、主分支保护和非版本化 PR policy 仍继续生效。
- 既有 Git tags、GitHub Releases、release manifests 与 Git notes 保持历史可读，但不再是新构建或新发布的版本输入。

## Replacements / Background

- 当前最后已发布 tag `v0.22.0` 是 Version File migration baseline。首个 `type:patch + channel:stable` Release Commit 使用 `0.22.1`；exact intent 在其 Release Commit 中锁定所给文本，之后的每个源提交都有独立版本边界。
- 一源提交一版本要求 Release Commit 在下一个源提交前完成；该顺序替代了以多个连续 `main` 提交合并发布的设计。
- 自动 patch、受控 exact、RC promotion 与现有 `GITHUB_TOKEN` / `github-actions` 权限边界由 ADR 0004 固化；Label Gate 和 snapshot 继续仅保存发布意图。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- [ADR 0003](../../adr/0003-version-file-is-the-product-version-source.md)
- [ADR 0004](../../adr/0004-release-commit-version-control.md)
