# Flux Purr Worktree Bootstrap 决策记录（#22222）

## Decision Log

- 采用 shared Git hooks + repo-local bootstrap script 作为 linked worktree onboarding contract。
- 保留项目特例：`post-checkout` 自动安装 repo-managed 开发依赖，但系统前置仍为 detect-only。
- `Worktree bootstrap` smoke 将进入 PR required checks，避免 contract 只存在于脚本。
- shared hook wrapper 显式标记 repo-managed hook，避免在重复安装时把旧的 shared hook 快照链回自己。
- Cargo prewarm 继续使用 `cargo fetch --locked` 作为只读预热探针；当仓库未声明可复用的 workspace lockfile 时，该层降级为 warning-only。
