# Flux Purr PR 标签发布与主分支保护实现状态（#r9k3m）

## Current Coverage

- `CI PR` 负责 PR 上的 firmware 和 web checks。
- `Label Gate` 负责 release intent 标签检查，并把 intent 绑定到 PR head SHA 写入冻结 marker。
- `CI Main` 负责 `main` 上的非抢占式验证和 release snapshot 写入。
- `Release Web` 与 `Release Firmware` 从 release snapshot 导出发布意图。
- `.github/quality-gates.json` 声明主分支保护、签名提交、required checks，以及 owner PR 不强制 approval 的 review policy。

## Validation

- `.github/scripts/test-release-labels.sh` passes.
- `.github/scripts/test-version-scripts.sh` passes.
- `.github/scripts/check-quality-gates.py` passes.
- `python3 -m py_compile .github/scripts/release_snapshot.py .github/scripts/check-quality-gates.py` passes.
- Existing firmware/web checks pass locally.

## Rollout Notes

- GitHub 远端 branch protection / ruleset 需要按 `.github/quality-gates.json` 对齐。
- 如果当前自动化工具不能写入 GitHub ruleset，PR 应明确保留远端对齐说明。
