# Release 失败通知接入

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，主题局部生命周期见 `./HISTORY.md`。

## Context and Scope

Flux Purr 的 `Release Product` 需要在发布失败时发送可恢复的通知，并保留一个不带输入的手动 smoke 入口。通知 wrapper 属于 Flux Purr 的 release automation，发布特有的上下文由调用方生成；Oidrune 负责 OIDC gateway handoff 与下游 Telegram normalization。

### In scope

- `.github/workflows/notify-release-failure.yml` 的 `Release Product` 失败触发、`main` 过滤和 `workflow_dispatch` smoke 路径。
- 调用固定 SHA 的 `IvanLi-CN/oidrune/.github/workflows/notify.yml` reusable workflow。
- caller-owned summary、`id-token: write` 权限和旧 Telegram secret wiring 的移除。

### Out of scope

- `Release Product` 的标签、版本、产物和 recovery 行为。
- Oidrune gateway、OIDC audience、Telegram provider 或生产通知服务的配置。
- 真实 `workflow_dispatch` smoke notification。

## Requirements

- **REQ-JB85U-TRIGGER:** `notify-release-failure.yml` MUST trigger from `workflow_run` for `Release Product`, retain `types: [completed]` and `branches: [main]`, and notify only when the workflow-run conclusion is `failure`.
- **REQ-JB85U-SMOKE:** The wrapper MUST retain a no-input `workflow_dispatch` smoke job with a failure outcome and a caller-generated smoke title.
- **REQ-JB85U-PIN:** Both notification jobs MUST call `IvanLi-CN/oidrune/.github/workflows/notify.yml@e48822f99c6402a753ed86557ea029754cbab20b` and MUST omit gateway URL and OIDC audience overrides.
- **REQ-JB85U-PERMISSION:** Each Oidrune caller job MUST grant `id-token: write` and MUST NOT pass `SHOUTRRR_URL` or another Telegram secret.
- **REQ-JB85U-SUMMARY:** The caller MUST provide the complete Oidrune `summary`. The failure summary MUST include a failure title, project name, status, target SHA, run URL, and the existing PR, label, version, artifact, and recovery context. The smoke summary MUST include its smoke title, project name, failure status, target SHA, and current run URL.

## Verification

- **VER-JB85U-WORKFLOW:** The workflow contract test and `actionlint` cover: REQ-JB85U-TRIGGER, REQ-JB85U-SMOKE, REQ-JB85U-PIN, REQ-JB85U-PERMISSION, REQ-JB85U-SUMMARY.
- **VER-JB85U-STATIC:** `git diff --check`, shell syntax checks, and the release workflow fixture test cover: REQ-JB85U-TRIGGER, REQ-JB85U-SMOKE, REQ-JB85U-PIN, REQ-JB85U-PERMISSION, REQ-JB85U-SUMMARY.

## Related ADRs

- [0005-pr-local-version-preparation](../../adr/0005-pr-local-version-preparation.md)
