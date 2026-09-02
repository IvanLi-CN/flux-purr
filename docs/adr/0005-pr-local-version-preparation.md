# PR-local version preparation

## Status

Accepted

Supersedes [0004-release-commit-version-control](0004-release-commit-version-control.md).

## Context

The previous release chain created a VERSION-only commit after a source commit
was already on `main`, published assets from that child, then attempted to
fast-forward protected `main`. Repository rules correctly reject that direct
write. It left a possible split between the published version and the
`VERSION` value visible to development builds.

The repository must retain normal pull-request protection and cannot introduce
a new bypass actor, credential, secret, variable, or GitHub Environment.

## Decision

- Root `VERSION` remains the only product-version source. Development reads it
  and displays `nextPatch(VERSION)-dev.<short-sha>` without modifying it.
- A product PR first completes `Validate PR labels` and the full PR CI matrix.
  The trusted `Prepare product version` workflow then appends one
  VERSION-only preparation commit to that same in-repository PR branch. It
  writes `Release-Source-SHA`, `Product-Version`, and the validated label
  intent into commit trailers. It never creates another PR and never writes
  `main`.
- `type:patch + channel:stable` writes `nextPatch(VERSION)`. Major, minor, and
  RC intent require an explicit exact value supplied to the preparation
  workflow; labels only select that operation and never calculate a number.
- `Release completion` accepts a product PR only when its head is that prepared
  commit, its source is based on the current `main`, the preparation is
  consistent with the current validated labels, and the source commit has the
  full successful PR check set. `type:docs` and `type:skip` retain their
  existing no-product-release path.
- Product PRs use a merge commit. The merged `main` commit must have the
  preparation commit as its second parent and the same tree. `CI Main` performs
  structural validation for that integration; the full matrix has already run
  once for the source PR head.
- `Release Product` builds, tags, publishes, and verifies from the merged
  `main` commit. Its `sourceSha`, manifest, tag target, and build identity all
  refer to that commit. It performs no push to `main`.
- Before a preparation commit is written, the release controller reserves the
  derived `v<version>` name. Any existing tag blocks preparation. A recovery
  may reuse a tag only after proving that it points to the same merged `main`
  commit; it never changes the version calculation.
- If publication fails after merge, `main/VERSION` remains the committed exact
  version. Recovery accepts that same merged SHA and may only republish that
  identity. It cannot calculate a successor version or change `VERSION`.

## Consequences

The release version is locked before the normal PR merge, not after it. Normal
branch protection remains the only route into `main`; the existing workflow
token needs write access only to the already-open PR branch and to release
tags/assets. No bypass configuration is required.

An RC-to-stable transition cannot be a post-merge direct write under this
policy. It must be represented by a prepared stable product PR, so the stable
release has its own protected merge boundary and exact `VERSION` value.

## Alternatives considered

- A direct post-release push to `main` needs a protection bypass and is
  rejected by the repository rules.
- A separate version-only PR would add a PR solely to update version metadata.
- Retagging an RC commit as stable would give one source commit two product
  identities and make rollback provenance ambiguous.
