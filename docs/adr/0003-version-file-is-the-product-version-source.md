# Version File Is the Product Version Source

## Status

Accepted

Flux Purr declares its exact product SemVer only in the root `VERSION` file. A product tag, package manifest, PR label, release snapshot, and workflow input may record or request work, but none may establish the version consumed by a build, runtime, manifest, or release asset. For a normal release, `nextPatch(VERSION)` is the release version; development leaves `VERSION` untouched and displays `nextPatch(VERSION)-dev.<short-sha>`. The protected-merge transport for that value is defined by [ADR 0005](0005-pr-local-version-preparation.md).

## Consequences

- Every verified product-source PR with enabled release intent is a separate product release. A VERSION-only preparation commit is appended only after the source PR has passed its full checks, and the normal protected merge carries that version into `main`. `type:patch + channel:stable` permits the only automatic transition, `nextPatch(VERSION)`. `type:minor`, `type:major`, and `channel:rc` require a controlled exact value written once to `VERSION`. Labels decide release eligibility and whether exact input is required; they never calculate or parse a number. Git tags, package manifests, snapshots, and workflow channels cannot override that file.
- A preparation commit changes only `VERSION` and records the source SHA and validated release intent in trailers. `Release completion` rejects an unprepared product PR. The full matrix runs once for the source PR; `CI Main` structurally verifies the normal merge, and `Release Product` builds the merged `main` commit once. The existing workflow token never bypasses `main` protection.
- Tags are derived from the committed `VERSION` after the normal merge and point to that merge commit. Recovery rebuilds from the same main commit and never recomputes a version.
- Firmware, host tools, the Web bundle, CLI output, and runtime health endpoints must use one shared resolver. Package versions remain package metadata and must not be used as a product-version fallback.
