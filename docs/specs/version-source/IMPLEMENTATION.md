# Flux Purr 单一产品版本源实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实。

## Current Status

- Implementation: implemented in the version-source branch; rollout requires owner-side GitHub protection/App configuration.
- Lifecycle: active
- Catalog note: root `VERSION`, one-source-commit/one-product-release sequencing, and the release-completion gate are implemented in repository code and workflows.

## Coverage / rollout summary

- firmware `build.rs`, `flux-purr-devd` build script, CLI metadata, `/health`, local firmware bundle, Vite build metadata, release manifest, and release workflow all resolve product identity from root `VERSION` through `scripts/product-version.py`.
- Development mode derives `nextPatch(VERSION)-dev.<short-sha>` and never writes `VERSION`; release mode reads the file verbatim. Git tags and package manifests are not version inputs; PR labels and notes remain the frozen release-intent and queue input, never a numeric-version fallback.
- 当前 active GitHub ruleset 没有 bypass actor，不能让现有 workflow token 创建受保护 `main` Release Commit。仓库还同时保留 classic branch protection；两者会同时生效，单独为 ruleset 添加 App bypass 不足以放行 Release Commit。发布自动化必须使用受限的 GitHub App Integration bypass，并先将 classic protection 收敛到 ruleset；该远端配置由 owner 执行，不在仓库代码变更范围内。

## Required Rollout

1. 在版本源实现 PR 中一次性建立 migration baseline：新增 root `VERSION` 内容为 `0.22.0`，并加入 shared resolver、所有 build consumers、工作流替换和测试。该文件创建是迁移初始化，不是普通开发期版本 bump。
2. owner 将现有 classic branch protection 的保护迁移到 `main` ruleset 并移除 classic rule，随后安装专用 release GitHub App，配置最小 `contents: write` 权限、受保护 environment secret，并把该 App 加入 ruleset 的 `always` bypass list。同步 ruleset 与 `.github/quality-gates.json`：保留 `Validate PR labels`，并同时要求 `Release completion`、Firmware、DEVD、Web 和 Worktree checks。
3. 合并实现 PR 后，`CI Main` 为该唯一源提交完成完整验证。release controller 创建 child Release Commit，写入 `VERSION=0.22.1`，从它发布并校验 `v0.22.1`，最后才将 `main` fast-forward 到该 commit。
4. 之后每个产品源提交严格重复“CI Main -> staged Release Commit -> assets/tag -> fast-forward main -> next merge”序列；release commit push 只执行结构验证或被路径规则跳过，绝不重新运行完整矩阵。
5. 在第一次成功发布后，继续调用 Label Gate 和 release intent snapshot；仅停止旧的 tag/manifest 版本计算脚本。更新 README、HTTP release-manifest 文档、quality-gates declaration 和旧规格的 implementation coverage。

## Remaining Gaps

- `VERSION` and the strict resolver are checked by `scripts/test-product-version.py`.
- `release_chain.py` enforces a one-parent, VERSION-only Release Commit with `Release-Source-SHA` and `Product-Version` trailers; recovery and RC promotion preserve that identity.
- `release_completion.py` rejects ordinary PR VERSION changes, permits only the exact migration baseline, and verifies the latest main release tag and published assets.
- `CI Main` runs the full matrix only for source commits; VERSION-only Release Commits use `release-commit.yml` structural validation. Product assets are built once from R before main is advanced.
- Label Gate 和 release intent snapshot 是当前可执行发布路径；旧的 tag/manifest 版本计算已退役。标签仍是发布意图 source of truth，但数字产品版本始终来自 `VERSION`。

## Related Changes

- [ADR 0003](../../adr/0003-version-file-is-the-product-version-source.md)
- [Version File contract](./contracts/version-file.md)
- [旧 label 发布规格](../r9k3m-pr-label-release-protection/SPEC.md)

## References

- `./SPEC.md`
- `./HISTORY.md`
