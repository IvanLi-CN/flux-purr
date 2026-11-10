# ADR 0025: Adopt Manual version release delivery

- Status: Accepted
- Date: 2026-11-10
- Supersedes: [ADR 0005](0005-pr-local-version-preparation.md), [ADR 0006](0006-tag-reservation-and-legacy-release-reconciliation.md) for the active release path

## Context

Flux Purr previously used PR labels to select release intent, created a VERSION-only commit on each product PR, and required completion checks to reconstruct that identity after merge. That path coupled mutable PR metadata to release eligibility, forced every product PR to prepare a version before the owner chose the actual release operation, and left multiple recovery branches to maintain.

The product still needs one version fact for firmware, devd, CLI, Web, bundles, manifests and GitHub Releases. It also needs protected PR/main boundaries, exact source provenance, RC support, artifact reuse across EdgeOne targets, and a recovery path that cannot silently allocate a successor version.

## Decision

Adopt a target-only `Manual version release delivery` contract:

1. `Release Product` is entered only through `workflow_dispatch`.
2. The caller supplies one owner-confirmed `release_target`: `patch`, `minor`, `major` or `rc`; the caller cannot supply a canonical version, tag or source SHA override.
3. Before dispatch, the Agent reads current `main`, the trusted stable baseline and changed paths, then suggests intent and expected version to the owner. The suggestion is conversational only. The owner confirms intent, and the workflow independently resolves the canonical version.
4. The workflow fixes current `main` exact SHA as the release source, derives the highest reachable stable final tag as baseline, and keeps root `VERSION` as the product build fact. It writes the resolved version only in the runner workspace after source identity is fixed.
5. `patch`, `minor` and `major` advance the stable baseline. `rc` advances the next stable baseline with the next unused `rc.N`; it does not advance the stable baseline.
6. A publish reserves and verifies an annotated `v<version>` tag before building. A recovery accepts only an existing release tag pointing at the exact current source SHA and reuses `<tag>@<sha>` as the identity. The old `commit_sha` recovery input and label/preparation recovery paths are removed.
7. Changes outside the documentation allowlist require a product Release by default. A change set containing both documentation and product files is a product release. Documentation-only changes skip product publication explicitly.
8. Release, Deploy and Notify remain separate stages. Each artifact is built once; EdgeOne production/demo reuse the verified Web archives and failure notification reports the same release identity and recovery target.
9. PR review, required checks, signed commits and PR-only main protection remain required. Manual release never writes `main`.

## Consequences

### Positive

- Release intent is explicit and owner-controlled without making the owner calculate a version.
- Workflow retries can be reasoned about from one tag@SHA identity.
- Multiple merged product changes can be released together from one exact main boundary while preserving one canonical VERSION for all artifacts.
- Labels, PR-local VERSION commits and historical recovery metadata no longer participate in new release decisions.

### Negative

- A manual dispatch is required for every non-document product change.
- The workflow must maintain a trusted baseline/tag inventory and recompute the version at dispatch time.
- RC and same-identity recovery semantics require more contract tests than a single patch-only path.
- Real GitHub Release and EdgeOne behavior cannot be proven by local tests and remains a post-merge acceptance task.

## Rejected alternatives

- **Keep PR label release:** rejected because labels are mutable PR metadata and couple release eligibility to review-time state.
- **Keep PR-local VERSION preparation:** rejected because it creates an unnecessary commit and separate recovery identity before the owner confirms the release operation.
- **Accept an exact version input:** rejected because version calculation and collision handling belong to the workflow's Version Policy; the owner chooses intent, not a number.
- **Use SemVer change governance as the target:** rejected. Compatibility assessment is optional and cannot select release intent, eligibility or RC serial.

## Verification

- `.github/scripts/manual_release.py` and `.github/scripts/test-manual-release.sh` cover target resolution, RC, documentation classification, collision and same-identity recovery.
- `.github/scripts/test-release-workflows.sh` and `.github/scripts/check-quality-gates.py` enforce the active workflow and quality-gate contract.
- `docs/specs/manual-version-release-delivery/SPEC.md` is the current implementation-facing contract.
