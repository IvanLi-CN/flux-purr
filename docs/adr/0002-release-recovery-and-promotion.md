# Release recovery and pre-release promotion

## Status

Accepted

## Context

Product release intent is frozen in a Git notes snapshot so a later PR-label change cannot change the version or channel selected for a commit. A failed release run can therefore be rebuilt from its snapshot, but an RC-to-stable publication must change the product channel and tag without mutating that source snapshot. The release workflow also builds a host-side firmware bundle tool on Ubuntu, so its Linux system dependency must be explicit and shared with the other host-tool jobs.

## Decision

Keep the release snapshot immutable and expose two explicit manual operations:

- `recover` reads an existing enabled snapshot and publishes its original target, channel, effective version, and tag.
- `promote` accepts an enabled RC snapshot on `main` whose CI Main run produced the snapshot, and projects the same source SHA and effective version to a stable tag.

Promotion state is stored separately in `refs/notes/release-promotions`. A schema-v1 record contains the candidate SHA, canonical digest of the source snapshot, candidate tag, source channel, stable channel, effective version, and stable tag. A repeated operation must match the existing record byte-for-byte. A stable tag may be reused only when the matching promotion record exists and the tag points at the candidate; a conflicting tag fails.

The manual workflow accepts only an operation and a `main` commit SHA. It derives all release identity fields from the snapshot or promotion record, never from manual version, tag, channel, or mutable PR labels. The existing automatic CI Main path continues to resolve the original snapshot. Linux jobs that build `flux-purr-devd` reuse one composite action that installs `pkg-config` and `libudev-dev` before the locked build.

## Consequences

Any qualified RC can be promoted to a stable release without requiring that the RC GitHub Release was already public. Recovery and promotion remain auditable, source-bound, and retryable, while the original release intent remains unchanged. The release workflow gains one additional notes ref and explicit operation validation. Existing `export` consumers and automatic snapshot publication remain compatible.

## Alternatives considered

- Mutating an RC snapshot to stable would destroy the original release intent and make the release history ambiguous.
- Requiring a new stable PR for every RC would prevent a qualified pre-release from being promoted directly and add an unnecessary product versioning step.
- Re-running the failed workflow without changing its build environment would reproduce the deterministic `libudev.pc` failure.
