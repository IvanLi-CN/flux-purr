# PR label release protection implementation（已退役）

活动实现已迁移到 `.github/workflows/release.yml`、`.github/scripts/manual_release.py` 和 `.github/scripts/test-manual-release.sh`。本目录不再声明 Label Gate、release preparation 或 Release completion 的实现状态。

质量门禁现在由 `.github/quality-gates.json` 声明 CI PR、CI Main 和 Release Product 的活动 workflow；失败通知由 `notify-release-failure.yml` 消费 release identity，而不是 PR labels。
