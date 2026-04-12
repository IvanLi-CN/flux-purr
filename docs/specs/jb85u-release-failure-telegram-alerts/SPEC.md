# jb85u · Release 失败 Telegram 告警接入

## Summary
- 为 `Release Web` 和 `Release Firmware` 增加统一的 repo-local notifier wrapper，复用共享 Telegram 告警 workflow。
- 保留 repo-local `workflow_dispatch` smoke test，用于 secret 轮换和链路自检。
- 保持现有 Web/Firmware 发布逻辑、标签与产物行为不变。

## Scope
- 新增 `.github/workflows/notify-release-failure.yml`。
- 监听 `Release Web` 与 `Release Firmware` 的失败结果并转发 Telegram 告警。
- 提供一个无输入的手动 smoke test 入口。

## Acceptance
- `Release Web` 或 `Release Firmware` 任一失败时，wrapper 都会自动发送 Telegram 告警。
- 告警首行必须是 Emoji + 状态 + 项目名。
- `workflow_dispatch` smoke test 能在默认分支成功触发 Telegram 通知。
- wrapper 中的 `workflows:` 列表必须同时包含 `Release Web` 与 `Release Firmware`。
