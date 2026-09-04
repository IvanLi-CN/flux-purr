---
title: GitHub-native verified VERSION preparation commits
module: release-automation
problem_type: workflow
component: github-actions
tags:
  - github-actions
  - graphql
  - signed-commits
  - version-preparation
status: active
related_specs:
  - docs/specs/version-source/SPEC.md
---

# GitHub-native verified VERSION preparation commits

## Context

Flux Purr prepares a product release by appending one `VERSION`-only commit to
the existing in-repository pull request. The target branch ruleset requires
commits to be signed and verified, but release automation must not hold a
long-lived private signing key.

## Symptoms

An action that creates a local commit and pushes it needs a GPG private key and
passphrase to satisfy the signed-commit ruleset. If those secrets are absent,
the preparation job fails after the VERSION staging logic has run. A job can
still appear successful when it detects an existing preparation and skips the
signing step, which does not prove that fresh preparation works.

## Root cause

Local Git signing requires the signing key to exist on the runner. The
`GITHUB_TOKEN` already grants the repository-scoped write capability, but it
does not provide a local private key for `git commit -S`.

## Resolution

Use the GitHub GraphQL `createCommitOnBranch` mutation with the existing
`GITHUB_TOKEN` and `contents: write` permission. Pass the PR branch, the exact
source SHA as `expectedHeadOid`, the staged `VERSION` contents, and the release
message/trailers. GitHub creates the child commit and automatically signs it
when supported. A separate verification step runs for both `created` and
`existing` preparation states. It verifies the branch head, the VERSION-only
parent/diff/trailer contract, and the REST commit `verification.verified` field,
with a bounded retry for verification propagation. A retry after a commit was
created but its verification has not propagated therefore fails closed instead
of treating the commit as already complete.

This preserves the PR-local commit model, avoids a new App or secret, and keeps
the ruleset enabled. The local staging commit remains an untrusted payload
source; only the GitHub-created commit is pushed to the PR branch and consumed
by later release checks.

## Guardrails / Reuse notes

- Keep the pre-mutation `git ls-remote` check and GraphQL `expectedHeadOid` so a
  concurrent PR update fails instead of being overwritten.
- Treat GraphQL errors, a missing returned OID, a changed remote branch head,
  a malformed preparation commit, or an unverified signature as hard failures.
- Do not replace the mutation with the Git Database REST API unless the caller
  supplies and protects an explicit detached signature; that path does not
  provide GitHub-native signing by itself.
- Do not remove the signed-commit ruleset or weaken the VERSION-only and source
  ancestry checks to avoid credential setup.

## References

- [GitHub GraphQL `createCommitOnBranch`](https://docs.github.com/en/graphql/reference/commits)
- [GitHub Actions `GITHUB_TOKEN`](https://docs.github.com/en/actions/concepts/security/github_token)
- [Version source specification](../../specs/version-source/SPEC.md)
