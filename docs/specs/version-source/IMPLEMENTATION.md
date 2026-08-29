# Flux Purr 单一产品版本源实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实。

## Current Status

- Implementation: 未开始
- Lifecycle: active
- Catalog note: root `VERSION`、一源提交一产品版本和 release-completion gate 尚未落地。

## Coverage / rollout summary

- 当前 firmware `build.rs` 在未注入 `FLUX_PURR_FIRMWARE_VERSION` 时回退到 `CARGO_PKG_VERSION`；`tools/flux-purr-devd` 的 `/health` 直接返回 `CARGO_PKG_VERSION`。相关 Cargo/NPM manifests 目前均含 `0.1.0`，不能作为产品版本事实。
- 当前 local firmware bundle 从 Git tag 推导开发版本，release workflow 从 PR labels、Git notes snapshot 和 tag baseline 推导 effective version。它们都将被 shared resolver 和 `VERSION` 取代。
- 当前 active GitHub ruleset 没有 bypass actor，不能让现有 workflow token 创建受保护 `main` Release Commit。仓库还同时保留 classic branch protection；两者会同时生效，单独为 ruleset 添加 App bypass 不足以放行 Release Commit。发布自动化必须使用受限的 GitHub App Integration bypass，并先将 classic protection 收敛到 ruleset；该远端配置由 owner 执行，不在仓库代码变更范围内。

## Required Rollout

1. 在版本源实现 PR 中一次性建立 migration baseline：新增 root `VERSION` 内容为 `0.22.0`，并加入 shared resolver、所有 build consumers、工作流替换和测试。该文件创建是迁移初始化，不是普通开发期版本 bump。
2. owner 将现有 classic branch protection 的保护迁移到 `main` ruleset 并移除 classic rule，随后安装专用 release GitHub App，配置最小 `contents: write` 权限、受保护 environment secret，并把该 App 加入 ruleset 的 `always` bypass list。同步 ruleset 与 `.github/quality-gates.json`：移除 `Validate PR labels`，加入 release-completion check、Firmware、DEVD、Web 和 Worktree checks。
3. 合并实现 PR 后，`CI Main` 为该唯一源提交完成完整验证。release controller 创建 child Release Commit，写入 `VERSION=0.22.1`，从它发布并校验 `v0.22.1`，最后才将 `main` fast-forward 到该 commit。
4. 之后每个产品源提交严格重复“CI Main -> staged Release Commit -> assets/tag -> fast-forward main -> next merge”序列；release commit push 只执行结构验证或被路径规则跳过，绝不重新运行完整矩阵。
5. 在第一次成功发布后，删除或停止调用 label gate、release snapshot、promotion snapshot 与旧的 version computation scripts；更新 README、HTTP release-manifest 文档、quality-gates declaration 和旧规格的 implementation coverage。

## Remaining Gaps

- 新增 `VERSION` file-format parser、Rust build-script bridge、Bash/Python/Vite consumer 和 deterministic test fixtures。
- 为 `devd` 与 CLI 增加 product version constants，替换 `CARGO_PKG_VERSION` 运行时输出。
- 为 Web build 生成并校验 build-info metadata。
- 重写 `CI Main` / `Release Product` 的 target resolution、release commit creation、recovery 和 recursion guards。
- 实现 release-completion required check，并在 GitHub ruleset 中实际启用。
- 移除 label/snapshot 版本路径、对应 workflow jobs、tests 和 README 文案；保留 labels 仅作非版本化 issue/PR 分类时不读取它们。

## Related Changes

- [ADR 0003](../../adr/0003-version-file-is-the-product-version-source.md)
- [Version File contract](./contracts/version-file.md)
- [旧 label 发布规格](../r9k3m-pr-label-release-protection/SPEC.md)

## References

- `./SPEC.md`
- `./HISTORY.md`
