# Squash-merged product release identity

## Status

Superseded by [ADR 0025](0025-manual-version-release-delivery.md).

## Context

The product release contract prepares a VERSION-only commit on the product PR before merge. The release verifier originally accepted only a two-parent merge whose second parent was that preparation commit. A GitHub squash merge can produce a protected main commit with one parent while preserving the prepared commit message and release metadata. Treating that commit as an ordinary non-release merge leaves a valid product version on `main` without a recoverable publication path.

## Decision

`Release Product` accepts a one-parent squash-merged product commit only when all of the following are true:

- The commit message contains exactly the complete release metadata set: `Release-Source-SHA`, `Product-Version`, `Release-Intent-Type`, `Release-Intent-Channel`, `Release-Intent-Action`, and `Release-Intent-Components`.
- `Release-Source-SHA` is a full commit SHA and is an ancestor of the squash commit's parent.
- The squash commit differs from the recorded source in exactly `VERSION`.
- The committed `VERSION`, product version, and validated release intent agree.

The existing two-parent merge path remains unchanged. The squash path reports the main commit itself as the release and preparation identity, so tag, manifest, assets, and recovery remain bound to the immutable merged SHA.

## Consequences

Protected GitHub squash merges can be published and recovered without creating a second version commit or rewriting `main`. A squash commit with incomplete metadata, a foreign source, a non-VERSION diff, or mismatched version/intent is rejected. Ordinary one-parent commits remain non-release commits when they do not carry release metadata.

## Alternatives considered

- Requiring a new merge-only PR would add an unrelated protected boundary and delay publication of the already merged product.
- Retagging the prepared commit would violate the contract that release tags point at the merged `main` commit.
- Accepting any one-parent commit containing a version trailer would weaken source, diff, and intent provenance and is rejected.
