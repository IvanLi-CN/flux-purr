# Release Commit version control

## Status

Accepted

Supersedes [0002-release-recovery-and-promotion](0002-release-recovery-and-promotion.md).

## Context

The label-release workflow must remain the required PR intent gate, but its
snapshot cannot determine a product number. Product code must retain a single
version source, and each source commit needs an independently recoverable
release boundary without rerunning CI or coalescing commits.

## Decision

The root `VERSION` file is the only product version source.

- `type:patch + channel:stable` is the only automatic numeric transition. The
  release controller creates a child Release Commit that writes
  `nextPatch(VERSION)`.
- `type:minor`, `type:major`, and `channel:rc` preserve their Label Gate and
  frozen-snapshot requirements, but wait for `operation=exact`. That operation
  writes one valid, strictly newer exact VERSION value to the Release Commit;
  its stable/RC form must match the frozen channel label. Labels and snapshots
  do not parse, increment, or override a numeric version.
- Recovery rebuilds and publishes the same existing Release Commit. Promotion
  takes an RC Release Commit and creates a new stable Release Commit that
  removes only its RC suffix. Tags and assets bind to the new commit; no
  promotion note or retagging of the RC commit is used.
- `CI Main` performs the single structural validation for a Release Commit and
  skips the full matrix. `Release Product` observes that validation and exits,
  so product assets are built once for each source commit.
- A dedicated GitHub App with only `contents: write` performs release writes
  and is the sole ruleset bypass actor. The workflow's `GITHUB_TOKEN` remains
  read-only for release-intent lookup. No GitHub Environment is introduced.

## Consequences

The exact release version is immutable once its Release Commit exists. A
failed publication leaves that candidate available for recovery and keeps the
release-completion gate closed. The owner must install the App, configure
`RELEASE_APP_ID` and `RELEASE_APP_PRIVATE_KEY`, make it the only always-bypass
actor, and remove overlapping classic branch protection before rollout.

## Alternatives considered

- Computing major, minor, RC, or channel versions from PR labels would create
  a second product-version source.
- Reusing an RC commit for its stable tag would give one commit two product
  identities and weaken rollback provenance.
- Using `GITHUB_TOKEN` for protected-main writes cannot satisfy the ruleset
  boundary.
