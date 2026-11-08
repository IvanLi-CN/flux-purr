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
- `CI Main` does not upload deployable Web artifacts. `Release Product` builds the production and public-demo archives once, publishes and verifies them, then uses one shared deployment helper for each exact archive and corresponding EdgeOne project. The helper makes bounded retries only for explicitly recognized transport-level `check and set region` endpoint-preflight failures observed before upload or deployment creation, preserves CLI success or failure status, writes the release marker only after a successful deployment, and refuses to retry when output capture is incomplete or remote state is ambiguous. Recovery continues to use the marker as a skip guard; it does not infer remote deployment state when marker upload fails.
- Tag reservation is enforced before preparation and again before release assets are built. A foreign existing tag fails closed; recovery may reuse only a tag pointing at the exact merged main SHA.
- The active signed-commit ruleset is satisfied by GitHub's verified signature for the `createCommitOnBranch` preparation commit. This implementation requires no release signing key, secret, variable, App, Environment, or bypass.

## Required Repository Settings

1. Keep `main` protected by the existing pull-request, signed-commit, and required-check rules.
2. Add the already-declared `Release completion` check to the remote required checks without removing `Validate PR labels`.
3. Use merge commits for product PRs. The release controller rejects a merge that does not preserve the prepared commit as its second parent and tree-equivalent provenance.

## Recovery Boundary

- A publication failure after the protected merge leaves the exact `VERSION` in `main`.
- `Release Product` recovery takes that same merged SHA and verifies the tag, manifest, and assets before republishing. It cannot calculate a new version or modify `VERSION`.
- A historical candidate that was tagged but never merged into `main` is audit-only. It cannot be used as a future version input or recovered by the new main-merge flow.
- The historical `v0.23.0` tag/release remains immutable and audit-only. The first complete release on this chain is the normal `0.23.1` migration reconciliation release.

## Validation

- `scripts/test-product-version.py` verifies strict VERSION parsing and development read-only identity.
- `.github/scripts/test-release-chain.sh`, `.github/scripts/test-release-preparation.sh`, `.github/scripts/test-release-completion.sh`, and `.github/scripts/test-release-workflows.sh` validate the preparation and recovery contracts.
- `.github/scripts/check-quality-gates.py` validates the repository-declared workflow checks.

## References

- `./SPEC.md`
- `./HISTORY.md`
- [ADR 0005](../../adr/0005-pr-local-version-preparation.md)
