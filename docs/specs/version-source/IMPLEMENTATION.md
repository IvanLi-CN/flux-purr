# Flux Purr 单一产品版本源实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 相关事实。

## Current Status

- Implementation: active repository workflow.
- Lifecycle: active.
- Catalog note: root `VERSION`, PR-local version preparation, and main-merge release recovery are implemented in repository code and workflows.

## Coverage

- firmware `build.rs`, `flux-purr-devd` build script, CLI metadata, `/health`, local firmware bundle, Vite build metadata, release manifest, and release workflow resolve product identity from root `VERSION` through `scripts/product-version.py`.
- Development mode derives `nextPatch(VERSION)-dev.<short-sha>` and never writes `VERSION`; release mode reads the file verbatim. Git tags and package manifests are not version inputs.
- `Label Gate` remains the required label validation. `Prepare product version` waits for that check and the full PR matrix, then writes a VERSION-only commit to the same PR branch with immutable source and label-intent trailers. It does not create a PR and does not write `main`.
- `Release completion` admits only a prepared product PR after it rechecks the prepared source commit's complete CI results. A normal merge commit carries the prepared tree into `main`; `CI Main` validates that relation and `Release Product` verifies both the merge relation and the preparation trailers before it builds, tags, publishes, deploys, and recovers from the merged main SHA. A non-product merge is a successful release skip, not a failed release.
- `CI Main` does not upload deployable Web artifacts. `Release Product` builds the production and public-demo archives once, publishes and verifies them, then deploys each exact archive once to its corresponding EdgeOne project. Release markers make recovery idempotent.
- No workflow bypass, App, secret, variable, or GitHub Environment is required. The workflow token writes only the already-open PR branch and the product tag/release assets.

## Required Repository Settings

1. Keep `main` protected by the existing pull-request, signed-commit, and required-check rules.
2. Add the already-declared `Release completion` check to the remote required checks without removing `Validate PR labels`.
3. Use merge commits for product PRs. The release controller rejects a merge that does not preserve the prepared commit as its second parent and tree-equivalent provenance.

## Recovery Boundary

- A publication failure after the protected merge leaves the exact `VERSION` in `main`.
- `Release Product` recovery takes that same merged SHA and verifies the tag, manifest, and assets before republishing. It cannot calculate a new version or modify `VERSION`.
- A historical candidate that was tagged but never merged into `main` is audit-only. It cannot be used as a future version input or recovered by the new main-merge flow.

## Validation

- `scripts/test-product-version.py` verifies strict VERSION parsing and development read-only identity.
- `.github/scripts/test-release-chain.sh`, `.github/scripts/test-release-preparation.sh`, `.github/scripts/test-release-completion.sh`, and `.github/scripts/test-release-workflows.sh` validate the preparation and recovery contracts.
- `.github/scripts/check-quality-gates.py` validates the repository-declared workflow checks.

## References

- `./SPEC.md`
- `./HISTORY.md`
- [ADR 0005](../../adr/0005-pr-local-version-preparation.md)
