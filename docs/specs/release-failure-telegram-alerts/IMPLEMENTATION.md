# Release 失败通知实现状态

## Current Coverage

- `.github/workflows/notify-release-failure.yml` listens for failed `Release Product` runs on `main` and retains a no-input manual `workflow_dispatch` smoke path.
- Both notification jobs call Oidrune's `notify.yml` reusable workflow at commit `e48822f99c6402a753ed86557ea029754cbab20b`.
- The caller supplies the complete notification summary. Failure context includes the repository, status, target SHA, run URL, release labels, selected version, artifact names, and recovery command; the manual path includes its smoke title and current run URL.
- Notification jobs grant `id-token: write` and omit gateway overrides so Oidrune resolves its default gateway and audience. No Telegram secret is passed.
- `.github/scripts/test-release-workflows.sh` asserts the pinned reusable workflow, permission boundary, removed secret wiring, failure filter, manual path, and caller-owned summary fields.
