# Flux Purr 真实控制平面运行时历史（#m8r4q）

## 2026-05-23

- 创建真实控制面 topic spec，把 PR #27 solution 从架构建议提升为 Flux Purr 的可实现 contract。
- 决策：`#hhwq8` 继续代表轻量 Web demo；真实 transport work 由本 spec 承接，避免把 demo spec 扩张成全量后台。
- 决策：真实控制面优先收敛在 Web -> native `devd` -> USB JSONL；direct `net_http` 只有在固件 HTTP server 真正实现并验证后才能声明为当前能力。
- 决策：无真机环境不得声明 USB/WiFi/flash 已完成硬件验证；可用 host tests、mock serial、devd dry-run 和 Web evidence 覆盖可验证部分。

## 2026-05-30

- 原型线 `th/real-control-plane-devd` 被拆回可合并 foundation：保留长期 contract、native `devd` crate、daemon smoke runner、devd validation hook 和可复用 solution guardrails。
- 决策：旧原型中的 Web UI、firmware USB adapter、hardware smoke 结果和视觉证据不作为 foundation 当前事实写入实现状态；这些内容按后续 firmware/Web rollout 拆分交付。
- 决策：owner-facing screenshot assets 不随 foundation 入库；视觉证据只在实际 UI 变更重新验证后写入 spec。
