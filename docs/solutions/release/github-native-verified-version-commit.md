---
title: GitHub-native verified VERSION preparation commits (superseded)
module: release-automation
problem_type: workflow
component: github-actions
tags:
  - github-actions
  - graphql
  - signed-commits
  - version-preparation
status: superseded
related_specs:
  - docs/specs/version-source/SPEC.md
  - docs/specs/manual-version-release-delivery/SPEC.md
---

# GitHub-native verified VERSION preparation commits (superseded)

> 本 solution 保留为历史背景。当前发布链路不再创建或消费 PR-local VERSION preparation commit；请使用 [Manual version release delivery](../../specs/manual-version-release-delivery/SPEC.md)。

## Context

Flux Purr historically prepared a product release by appending one `VERSION`-only commit to an in-repository pull request. The target branch ruleset required signed and verified commits, but release automation did not hold a long-lived private signing key.

## Historical resolution

The former workflow used GitHub GraphQL `createCommitOnBranch` with `GITHUB_TOKEN`, `expectedHeadOid`, and a VERSION-only diff so GitHub could create a verified commit without a local signing key. It verified the branch head, parent/diff/trailer contract, and REST `verification.verified` field before later release checks consumed that preparation.

## Current replacement

This mechanism is no longer an active implementation. The current workflow keeps root `VERSION` as the product fact but resolves the release version in the manual release runner workspace after fixing the exact `main` source SHA. Signed commits remain a repository quality gate for normal PR/main development, not a release-preparation mutation.

Publish reserves an immutable tag before building; same-identity recovery reuses only that tag@SHA identity. No PR label snapshot, VERSION-only preparation commit, or completion workflow is created for a new release.

## Guardrails

- Do not reintroduce a PR-local VERSION commit, label snapshot, or completion gate.
- Keep exact source SHA, tag collision, artifact provenance, and same-identity recovery checks fail closed.
- Do not remove the signed-commit ruleset or weaken PR/main protection to avoid release setup.

## References

- [Manual release specification](../../specs/manual-version-release-delivery/SPEC.md)
- [Version source specification](../../specs/version-source/SPEC.md)
- [ADR 0025](../../adr/0025-manual-version-release-delivery.md)
