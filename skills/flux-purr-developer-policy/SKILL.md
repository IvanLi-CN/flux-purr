---
name: flux-purr-developer-policy
description: Apply Flux Purr repository-wide developer policy. Use when Codex is acting as a developer inside this source tree and needs the repo-level rules before routing into developer operations, user operations, or other specialist skills.
---

# Flux Purr Developer Policy

This is the repo-level policy layer for developer-role agents working inside the Flux Purr source tree.

The human-facing source of truth is `docs/guides/flux-purr-developer-policy.md`. Repo `AGENTS.md` keeps routing and non-bypassable hard gates. `skills/flux-purr-developer-operations` remains the repository-internal developer operations/HIL counterpart to the installed or released user surface; it is not the repo-wide developer policy.

## Use this skill when

- The task is source-tree development in this repository: firmware, web, `devd`, docs, specs, release automation, or source-level validation.
- The agent is acting as a developer rather than an end user operating released tools.

## Required routing

- Read `docs/guides/flux-purr-developer-policy.md` first for the repo-wide developer rules and vocabulary.
- For `devd`, CLI, Web/native bridge, release automation, calibration, artifact verify, dry-run flash, real flash, mock HIL, or real hardware validation, also read `skills/flux-purr-developer-operations/SKILL.md`.
- For owner-facing operations through released `flux-purr`, released `flux-purr-devd`, or browser Web Serial, use `skills/flux-purr-user-operations/SKILL.md` instead of source-tree shortcuts.
- If Flux Purr uses IsolaPurr as the external HUB or bench source, read `$isolapurr-developer-operations` only for the external device side.

## Hard agent rules

- Respect repo `AGENTS.md` for routing and non-bypassable hard gates.
- Prefer repo checkout tooling; do not substitute global `flux-purr` or `flux-purr-devd` binaries for source-tree developer work.
- Finish non-hardware validation before any real device or HIL step.
- Do not perform flash, reset, serial write, selector changes, or port switching without an exact owner-authorized USB port.
- On the exact owner-authorized port, an owner-authorized Developer may use the paired `--skip-backup --confirm NO_EEPROM_BACKUP` bypass defined by the developer-operations skill. It skips ROM probing and EEPROM snapshot/archive creation regardless of application or ROM state; record that no EEPROM archive was created.
- Do not report mock smoke as hardware validation.
- Keep specs, solutions, and project docs aligned when behavior, contracts, release policy, or safety boundaries change.
- For a release-policy task, read `docs/specs/manual-version-release-delivery/SPEC.md` and `docs/specs/version-source/SPEC.md` before editing release automation or version consumers.
- The Agent must inspect the current `main` exact SHA, trusted stable baseline, and changed paths, then suggest a `release_target` (`patch`, `minor`, `major`, or `rc`) and expected version to the owner. The suggestion is conversational only; it is not persisted and is never a workflow input as a canonical version.
- The owner must confirm the intent before dispatching `Release Product`. The workflow accepts only the intent and operation, independently resolves canonical `VERSION`, and rejects caller-supplied version/tag/source overrides.
- Non-document changes normally require a real product Release. Documentation-only changes are limited to `docs/**`, `README*`, `CHANGELOG*`, Markdown/plain text, and documentation image resources; mixed changes require release.
- Do not use PR labels, `Label Gate`, PR-local VERSION preparation, `release_snapshot`, or the retired recovery entry. Same-identity recovery is allowed only when the immutable release tag points to the exact current source SHA.
- Do not change git remotes, upstreams, push targets, or credentials unless the owner explicitly asks.
