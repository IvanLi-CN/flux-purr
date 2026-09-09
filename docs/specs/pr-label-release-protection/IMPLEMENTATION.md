# Flux Purr PR 标签发布与主分支保护实现状态

## Current Coverage

The numeric version calculation is owned by [the version-source specification](../version-source/SPEC.md) and `release_chain.py`. Labels remain the required release-intent gate; after label validation and PR CI, `release_preparation.py` copies that intent into the VERSION-only preparation commit.

- `CI PR` runs the full firmware, DEVD, Web, and worktree matrix for a source head. A prepared VERSION commit receives structural validation only.
- `Prepare product version` accepts automatic events only when their `workflow_run` payload exposes a PR number; post-merge events without that source skip before checkout or PR resolution, while supplied sources remain subject to the open in-repository `main` PR checks.
- `Release completion` rejects an ordinary product PR until its prepared commit is present, matches its current labels and base, and its source parent has completed the full PR checks.
- `Label Gate` and `Release completion` use the PR number as a non-preemptive `queue: max` concurrency key. Each run obtains the current labels through the read-only GitHub API, so a stale event payload cannot determine the final required-check state.
- `CI Main` verifies that a normal merge preserves the prepared tree; `Release Product` builds, tags, publishes, and recovers from that merged SHA without pushing `main`.
- The workspace `Cargo.lock` remains tracked and every Ubuntu `flux-purr-devd` build uses the shared Linux serial dependency action before `--locked` builds.
- `.github/quality-gates.json` declares `Validate PR labels`, `Release completion`, and the source checks. The remote ruleset must require those existing checks, retain normal PR protection, and use merge commits for product PRs.
- The existing workflow `GITHUB_TOKEN` writes only the open PR branch and release tags/assets. No bypass actor, App, secret, variable, or GitHub Environment is used.

## Validation

- `.github/scripts/test-release-chain.sh`, `.github/scripts/test-release-preparation.sh`, `.github/scripts/test-release-completion.sh`, and `.github/scripts/test-release-workflows.sh` cover version preparation, intent metadata, gate behavior, and main-write removal.
- `.github/scripts/check-quality-gates.py` and Python compilation cover the workflow declarations.
- `.github/scripts/test-release-labels.sh` covers current-label snapshots overriding stale event payloads; `.github/scripts/test-release-workflows.sh` covers `queue: max`, removal of preemptive cancellation, and the trusted read-only API boundary.
