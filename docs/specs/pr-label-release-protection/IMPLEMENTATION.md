# Flux Purr PR 标签发布与主分支保护实现状态

## Current Coverage

The numeric version calculation is owned by [the version-source specification](../version-source/SPEC.md) and `release_chain.py`. Labels remain the required release-intent gate; after label validation and PR CI, `release_preparation.py` copies that intent into the VERSION-only preparation commit.

- `CI PR` runs the full firmware, DEVD, Web, and worktree matrix for a source head. A prepared VERSION commit receives structural validation only.
- `Release completion` rejects an ordinary product PR until its prepared commit is present, matches its current labels and base, and its source parent has completed the full PR checks.
- `CI Main` verifies that a normal merge preserves the prepared tree; `Release Product` builds, tags, publishes, and recovers from that merged SHA without pushing `main`.
- The workspace `Cargo.lock` remains tracked and every Ubuntu `flux-purr-devd` build uses the shared Linux serial dependency action before `--locked` builds.
- `.github/quality-gates.json` declares `Validate PR labels`, `Release completion`, and the source checks. The remote ruleset must require those existing checks, retain normal PR protection, and use merge commits for product PRs.
- The existing workflow `GITHUB_TOKEN` writes only the open PR branch and release tags/assets. No bypass actor, App, secret, variable, or GitHub Environment is used.

## Validation

- `.github/scripts/test-release-chain.sh`, `.github/scripts/test-release-preparation.sh`, `.github/scripts/test-release-completion.sh`, and `.github/scripts/test-release-workflows.sh` cover version preparation, intent metadata, gate behavior, and main-write removal.
- `.github/scripts/check-quality-gates.py` and Python compilation cover the workflow declarations.
