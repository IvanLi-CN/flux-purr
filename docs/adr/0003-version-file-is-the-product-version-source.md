# Version File Is the Product Version Source

## Status

Accepted

Flux Purr declares its exact product SemVer only in the root `VERSION` file. A product tag, package manifest, PR label, release snapshot, and workflow input may record or request work, but none may establish the version consumed by a build, runtime, manifest, or release asset. For a normal release, `nextPatch(VERSION)` is the release version; development leaves `VERSION` untouched and displays `nextPatch(VERSION)-dev.<short-sha>`. Each verified product-source commit receives its own Release Commit, which writes that same calculated value to `VERSION`; the controller builds and verifies the release from that commit before fast-forwarding `main` to it, and every release step reads the committed file again.

## Consequences

- Every CI-verified non-Release Commit on `main` with enabled release intent is a separate product release. The release controller must create its Release Commit before another product-source commit may merge; it must never coalesce multiple source commits into one version. `type:patch + channel:stable` permits the only automatic transition, `nextPatch(VERSION)`. `type:minor`, `type:major`, and `channel:rc` require a controlled `exact` operation: its input is written once to `VERSION` in the Release Commit, then all downstream work reads that file. Labels decide release eligibility and whether exact input is required; they never calculate or parse the numeric version. Git tags, package manifests, snapshots, and workflow channels cannot override that file.
- The release controller may create a Release Commit only after its source commit has passed CI and only when its diff is limited to `VERSION`. It tags, publishes, and verifies the release before fast-forwarding `main` to that commit. `main` therefore needs an enforced release-completion gate before the next product-source merge. A Release Commit receives only one structural validation in `CI Main`; the full matrix and product asset build run once for its source commit, and `Release Product` exits for the Release Commit push. A dedicated GitHub App with only `contents: write` is the sole ruleset bypass actor for these writes; no GitHub Environment is introduced.
- Tags are derived from the committed `VERSION` after the Release Commit and point to that commit. Recovery rebuilds from the same Release Commit and never recomputes a version.
- Firmware, host tools, the Web bundle, CLI output, and runtime health endpoints must use one shared resolver. Package versions remain package metadata and must not be used as a product-version fallback.
