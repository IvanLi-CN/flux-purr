# Flux Purr 真实控制平面运行时历史（#m8r4q）

## 2026-05-23

- 创建真实控制面 topic spec，把 PR #27 solution 从架构建议提升为 Flux Purr 的可实现 contract。
- 决策：本轮无真机时不阻塞 merge-ready；必须以 host tests、mock serial、devd dry-run 和 Storybook 证据覆盖可验证部分。
- 决策：`#hhwq8` 继续代表轻量 Web demo；真实 transport work 由本 spec 承接，避免把 demo spec 扩张成全量后台。
