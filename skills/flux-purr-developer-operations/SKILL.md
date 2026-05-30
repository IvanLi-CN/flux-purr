---
name: flux-purr-developer-operations
description: Run Flux Purr source-level developer operations safely. Use when Codex needs to work on devd, CLI, firmware/Web integration, release automation, mock or real HIL validation, artifact verification, real flash proof, or repository-specific hardware authorization gates.
---

# Flux Purr Developer Operations

Use this skill for source-tree development, verification, and HIL. It does not replace the repository `AGENTS.md`; all port authorization and release gates still apply.

## Default Tooling

- Build and test devd/CLI with `bun run check:devd` or `cargo test --manifest-path tools/flux-purr-devd/Cargo.toml`.
- Start the local daemon with `flux-purr-devd serve`; pass `--bind`, `--serial-port`, `--artifact-root`, `--allow-dev-cors`, and `--allow-real-flash` explicitly for test setups.
- Use released-style CLI paths even from source: `cargo run --manifest-path tools/flux-purr-devd/Cargo.toml --bin flux-purr -- ...`.
- Use `scripts/devd-hardware-smoke.py --device-id mock-fp-lab-01 --allow-mock-device` only for mock HTTP contract proof. Never report mock smoke as hardware validation.

## HIL Gate

- Stop after non-hardware validation and ask the owner to prepare hardware.
- Require an exact authorized USB port before any real device operation.
- If the authorized port disappears or re-enumerates to another path, stop and report evidence. Do not switch ports automatically.
- Real HIL should prove CLI-through-devd `identity`/`status`, runtime write/readback/restore, artifact verify, dry-run flash, real flash with `--allow-real-flash` and `--confirm FLASH`, reboot, and post-flash identity/status/events.
- Do not use `mcu-agentd` as the acceptance path for CLI/devd HIL unless the owner explicitly changes the plan.

## Release Work

- Use product tag `vX.Y.Z`; do not recreate `web/v...` or `fw/v...` workflows.
- Publish Web, firmware, host-tools, and `flux-purr-release-manifest-vX.Y.Z.json` on the same GitHub Release.
- Keep release manifest components explicit with `sha256`, `contentSha256`, `sourceSha`, `protocolVersions`, `changedSincePrevious`, and `updateReason`.
- PRs that touch hardware behavior, release policy, CLI/devd contracts, or user operations must update relevant specs, solutions, and project docs.
