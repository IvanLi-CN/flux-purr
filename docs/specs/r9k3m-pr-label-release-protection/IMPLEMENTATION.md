# Flux Purr PR 标签发布与主分支保护实现状态（#r9k3m）

## Current Coverage

- `CI PR` 负责 PR 上的 firmware 和 web checks。
- `Label Gate` 负责 release intent 标签检查，并把 intent 绑定到 PR head SHA 写入冻结 marker。
- `Label Gate` 需要 `pull-requests: write` 权限，确保 `pull_request_target` 运行能创建冻结 marker comment。
- `CI Main` 负责 `main` 上的非抢占式验证和 release snapshot 写入。
- `Release Product` 从 release snapshot 导出发布意图，并创建单一 product tag。
- `Release Product` supports explicit `recover` and `promote` dispatches; promotion records are stored under `refs/notes/release-promotions` and preserve the source snapshot.
- Publish retries verify an existing stable tag and Release manifest against the resolved source, version, channel, component identities, and asset hashes before reusing partial output.
- Release snapshot validation keeps historical schema-v1 component snapshots readable for version-baseline reconciliation while requiring newly generated snapshots to carry the single product record.
- The workspace `Cargo.lock` is tracked so all host-tool release matrix builds can honor `cargo ... --locked`.
- All Ubuntu jobs that build `flux-purr-devd`, including the firmware bundle job, reuse the local Linux serial dependency action to install `pkg-config` and `libudev-dev` before the locked build so `libudev-sys` has its declared system dependency in the clean runner.
- `.github/quality-gates.json` 声明主分支保护、签名提交、required checks，以及 owner PR 不强制 approval 的 review policy。

## Validation

- `.github/scripts/test-release-labels.sh` passes.
- Release resolver fixtures cover recovery, RC promotion, promotion-note mismatch, and stable-tag conflicts.
- Workflow static checks cover the manual operation choices and shared Linux serial dependency action.
- `.github/scripts/test-version-scripts.sh` passes.
- `.github/scripts/check-quality-gates.py` passes.
- `python3 -m py_compile .github/scripts/release_snapshot.py .github/scripts/product_release_manifest.py .github/scripts/check-quality-gates.py` passes.
- Existing firmware/web checks pass locally.

## Rollout Notes

- GitHub 远端 branch protection / ruleset 需要按 `.github/quality-gates.json` 对齐。
- 如果当前自动化工具不能写入 GitHub ruleset，PR 应明确保留远端对齐说明。
