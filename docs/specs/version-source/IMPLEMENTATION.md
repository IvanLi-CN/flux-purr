# Flux Purr 单一产品版本源实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实。

## Current Status

- Implementation: implemented in the version-source branch; rollout requires owner-side GitHub protection configuration.
- Lifecycle: active
- Catalog note: root `VERSION`, one-source-commit/one-product-release sequencing, and the release-completion gate are implemented in repository code and workflows.

## Coverage / rollout summary

- firmware `build.rs`, `flux-purr-devd` build script, CLI metadata, `/health`, local firmware bundle, Vite build metadata, release manifest, and release workflow all resolve product identity from root `VERSION` through `scripts/product-version.py`.
- Development mode derives `nextPatch(VERSION)-dev.<short-sha>` and never writes `VERSION`; release mode reads the file verbatim. Git tags and package manifests are not version inputs; PR labels and notes remain the frozen release-intent and queue input, never a numeric-version fallback.
- 当前 active GitHub ruleset 没有 `github-actions` bypass，不能让 release controller 使用现有 `GITHUB_TOKEN` 创建受保护 `main` Release Commit。仓库还同时保留 classic branch protection；两者会同时生效。owner 必须先将 classic protection 收敛到 ruleset、移除 classic rule，并将现有 `github-actions` integration（ID `15368`）设为唯一 always-bypass actor。发布流程不依赖新增 App、secret、variable 或 GitHub Environment；该远端规则配置由 owner 执行，不在仓库代码变更范围内。

## Required Rollout

1. 在版本源实现 PR 中一次性建立 migration baseline：新增 root `VERSION` 内容为 `0.22.0`，并加入 shared resolver、所有 build consumers、工作流替换和测试。该文件创建是迁移初始化，不是普通开发期版本 bump。
2. owner 将现有 classic branch protection 的保护迁移到 `main` ruleset 并移除 classic rule，将现有 `github-actions` integration（ID `15368`）设为唯一 always-bypass actor。同步 ruleset 与 `.github/quality-gates.json`：保留 `Validate PR labels`，并同时要求 `Release completion`、Firmware、DEVD、Web 和 Worktree checks。
3. 合并实现 PR 后，`CI Main` 为该唯一源提交完成完整验证。若冻结意图为 `type:patch + channel:stable`，release controller 创建 child Release Commit 并写入 `VERSION=0.22.1`；否则它等待受控 `exact` 写入版本文本。两种路径都从该 commit 发布并校验资产，最后才将 `main` fast-forward 到它。
4. 之后每个产品源提交严格重复“CI Main -> staged Release Commit -> assets/tag -> fast-forward main -> next merge”序列；Release Commit push 只由 `CI Main` 执行一次结构验证，绝不重新运行完整矩阵或产品发布。
5. 在第一次成功发布后，继续调用 Label Gate 和 release intent snapshot；仅停止旧的 tag/manifest 版本计算脚本。更新 README、HTTP release-manifest 文档、quality-gates declaration 和旧规格的 implementation coverage。

## Remaining Gaps

- `VERSION` and the strict resolver are checked by `scripts/test-product-version.py`.
- `release_chain.py` enforces a one-parent, VERSION-only Release Commit with `Release-Source-SHA` and `Product-Version` trailers; recovery and RC promotion preserve that identity.
- `release_completion.py` rejects ordinary PR VERSION changes, permits only the exact migration baseline, and verifies the latest main release tag and published assets.
- `CI Main` runs the full matrix only for source commits and performs the sole structural validation for a VERSION-only Release Commit. Product assets are built once from R before main is advanced.
- Label Gate 和 release intent snapshot 是当前可执行发布路径；旧的 tag/manifest 版本计算已退役。标签仍是发布意图 source of truth，但数字产品版本始终来自 `VERSION`。

## Related Changes

- [ADR 0003](../../adr/0003-version-file-is-the-product-version-source.md)
- [Version File contract](./contracts/version-file.md)
- [旧 label 发布规格](../r9k3m-pr-label-release-protection/SPEC.md)

## References

- `./SPEC.md`
- `./HISTORY.md`
