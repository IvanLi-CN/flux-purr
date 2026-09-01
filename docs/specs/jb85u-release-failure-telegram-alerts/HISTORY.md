# jb85u · Release 失败通知生命周期

## Lifecycle

- The release failure wrapper uses an Oidrune OIDC-authenticated reusable workflow as its notification transport.
- The caller-owned summary is the compatibility boundary: release-specific metadata remains generated in Flux Purr while Oidrune owns gateway delivery and Telegram normalization.

## Compatibility

- `Release Product` failure detection, `main` filtering, release context collection, recovery backfill guidance, and the manual smoke trigger remain repo-local behavior.
- The previous `SHOUTRRR_URL` secret-based reusable workflow call is not part of the current contract.
