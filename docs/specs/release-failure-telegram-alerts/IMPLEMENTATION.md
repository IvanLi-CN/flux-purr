# Release 失败通知实现状态

## Current Coverage

- `.github/workflows/notify-release-failure.yml` listens for failed `Release Product` runs on `main` and retains a no-input manual `workflow_dispatch` smoke path.
- Both notification jobs call Oidrune's `notify.yml` reusable workflow at commit `e48822f99c6402a753ed86557ea029754cbab20b`.
- The caller supplies the complete notification summary. Failure context includes the repository, status, exact main SHA, run URL, release tag/version when reserved, reservation identity, artifact names, and same-identity recovery target; the manual path includes its smoke title and current run URL.
- `Release Product` uploads a `release-identity` artifact before tag reservation. The notifier downloads and validates it before deriving the release tag, reservation, and source SHA; `merge_sha` remains the workflow-run main SHA while `source_sha` remains the recovery identity source.
- Notification jobs grant `id-token: write` and omit gateway overrides so Oidrune resolves its default gateway and audience. No Telegram secret is passed.
- `.github/scripts/test-release-workflows.sh` plus PR/Main `actionlint` assert the pinned reusable workflow, permission boundary, removed secret wiring, failure filter, manual path, identity artifact, and caller-owned summary fields.
