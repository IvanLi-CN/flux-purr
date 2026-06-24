# Flux Purr Worktree Bootstrap 决策记录（#22222）

## Decision Log

- 采用 shared Git hooks + repo-local bootstrap script 作为 linked worktree onboarding contract。
- 保留项目特例：`post-checkout` 自动安装 repo-managed 开发依赖，但系统前置仍为 detect-only。
- `Worktree bootstrap` smoke 将进入 PR required checks，避免 contract 只存在于脚本。
- shared hook wrapper 显式标记 repo-managed hook，避免在重复安装时把旧的 shared hook 快照链回自己。
- Cargo prewarm 改为在临时 workspace snapshot 中执行，既保留依赖预热，又避免在真实 checkout 留下 bootstrap 生成的 `Cargo.lock`。
