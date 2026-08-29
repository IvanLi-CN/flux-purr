# Flux Purr PR 标签发布与主分支保护实现状态（#r9k3m）

## Current Coverage

The numeric version calculation is now owned by [the version-source specification](../version-source/SPEC.md) and `release_chain.py`. The label/snapshot path remains active for release intent validation, mainline reconciliation, queue/backfill, and failure context; `Release completion` is an additional completion gate.

- `CI PR` and `CI Main` continue to own firmware, DEVD, Web, and worktree checks.
- `Release Product` creates one VERSION-only Release Commit per verified source commit, publishes from that commit, and fast-forwards `main` only after asset verification.
- Recovery reuses the durable `release/product-main` candidate; `type:patch + channel:stable` is the only automatic numeric transition, while minor, major, and RC labels require controlled `exact` before RC promotion can create a new stable Release Commit.
- The workspace `Cargo.lock` is tracked so all host-tool release matrix builds can honor `cargo ... --locked`.
- All Ubuntu jobs that build `flux-purr-devd`, including the firmware bundle job, reuse the local Linux serial dependency action to install `pkg-config` and `libudev-dev` before the locked build so `libudev-sys` has its declared system dependency in the clean runner.
- `.github/quality-gates.json` 声明主分支保护、签名提交、`Validate PR labels`、`Release completion` 及其他 required checks，以及 owner PR 不强制 approval 的 review policy。
- Release writes use the dedicated GitHub App token; `GITHUB_TOKEN` only reads frozen intent, and no GitHub Environment is required.

## Validation

- `.github/scripts/test-release-chain.sh`, `.github/scripts/test-release-completion.sh`, and `.github/scripts/test-release-workflows.sh` pass.
- Release-chain fixtures cover automatic patch staging, exact RC staging, stable promotion, migration, ordinary VERSION rejection, and workflow de-duplication.
- Workflow static checks cover the manual operation choices and shared Linux serial dependency action.
- `.github/scripts/check-quality-gates.py` passes.
- `python3 -m py_compile .github/scripts/release_chain.py .github/scripts/release_completion.py .github/scripts/product_release_manifest.py .github/scripts/check-quality-gates.py` passes.
- Existing firmware/web checks pass locally.

## Rollout Notes

- GitHub 远端 branch protection / ruleset 需要按 `.github/quality-gates.json` 对齐。
- 如果当前自动化工具不能写入 GitHub ruleset，PR 应明确保留远端对齐说明。
