# Flux Purr Worktree Bootstrap 实现状态（#22222）

## Current Coverage

- `scripts/install-hooks.sh` 统一接管 shared Git hooks 安装，记录主工作区，并在当前 worktree binary 不可用时回退到主工作区 `lefthook` binary。
- `lefthook.yml` 新增 shared `post-checkout` wiring，linked worktree 首次 checkout 会自动调用 `scripts/post-checkout-bootstrap.sh`。
- `scripts/bootstrap-dev.sh` 提供 `--manual` 与 `--auto` 两种模式，覆盖 root Bun、`web/` Bun、Cargo fetch 预热、shared hooks 刷新，以及系统前置 detect-only 报告。
- auto mode 使用 worktree-local stamp 对 root lock、web lock 和 Cargo manifest 指纹做增量判断；未变化时只输出 healthy 摘要。
- `scripts/test-worktree-bootstrap.sh` 用真实临时 linked worktree fixture 覆盖首次自动 bootstrap、重复 checkout skip、web lock 变化重跑、custom `core.hooksPath` preservation 与历史 revision no-op。
- smoke fixture 复制当前仓库内容时会显式排除忽略态 lockfile 与本地构建产物，避免把开发机残留文件误判成 bootstrap 回归。
- smoke fixture 的 `git worktree add` / detached checkout 会显式跳过 Git LFS smudge，避免 CI runner 因硬件模型大文件下载失败而误报 bootstrap contract 回归。
- `CI PR`、`CI Main` 与 `.github/quality-gates.json` 都已声明 `Worktree bootstrap` 为正式 gate。
- Cargo 预热在临时 workspace snapshot 中执行，因此 bootstrap 不会在真实 checkout 留下新的 `Cargo.lock`。

## Validation

- `bun run test:worktree-bootstrap`
- `python3 .github/scripts/check-quality-gates.py`
- `bun run check:web`
- `bun run check:devd`

## Rollout Notes

- 主工作区仍需先显式执行一次 `bun run bootstrap:dev` 作为 seed。
- 后续新 linked worktree 会自动尝试 repo-managed bootstrap，但所有失败都保持 warning-only；手动恢复入口固定为 `bun run bootstrap:dev` 与 `bun run worktree:setup`。
- Cargo prewarm 不会在真实 checkout 生成 `Cargo.lock`；当 Cargo 网络或 Xtensa toolchain 不健康时，它只输出 warning 与修复命令，不阻断 checkout 或 Bun 依赖恢复。
