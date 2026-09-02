# Tag Reservation and Legacy Release Reconciliation

## Status

Accepted

## Context

The root `VERSION` on the current `main` is `0.23.0`, but the historical
`v0.23.0` tag and GitHub Release point to an older, isolated release commit.
That tag is immutable history and cannot be moved to the current protected
`main` chain. The next preparation would otherwise write `0.23.1` successfully
and discover the same kind of ownership conflict only after building assets.

## Decision

- Product numbers continue to come only from the root `VERSION`. Tag names are
  derived identifiers and never inputs, fallbacks, or overwrite sources.
- Before any VERSION-only preparation commit is written, the controller checks
  that the derived `v<version>` ref is unclaimed. An existing ref is a hard
  failure and leaves VERSION, HEAD, and the PR branch unchanged.
- `Release Product` repeats the check before dependency installation and asset
  construction. A normal publish requires an absent tag; an explicit recovery
  may reuse an existing tag only when its peeled commit equals the requested
  merged `main` SHA. The post-build tag creation remains non-forced and repeats
  the ownership check to handle races.
- The historical `v0.23.0` tag, release, and assets remain unchanged and are
  audit-only. No retroactive `0.23.0` release is created. The first complete
  release on the current chain is the ordinary patch `0.23.1` prepared and
  merged by the corrective product PR.

## Consequences

Tag collisions fail before a preparation commit or expensive build can create
ambiguous release state. A failed publication still leaves the exact committed
VERSION on `main`, and recovery has a single existing identity to republish.
The migration preserves historical rollback and audit references while giving
future product PRs one immutable tag owner each.

## Alternatives considered

- Moving or deleting `v0.23.0` would rewrite published history and make existing
  rollback references unsafe.
- Discovering collisions only at tag push wastes the asset build and can leave
  a prepared PR that cannot be published.
- Treating an existing tag as the version source would violate the single
  VERSION contract and make concurrent release ownership ambiguous.
