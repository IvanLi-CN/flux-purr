---
name: flux-purr-developer-operations
description: Run Flux Purr repository-internal developer operations safely. Use when Codex is already operating under the repo-level developer policy and needs devd, CLI, firmware/Web integration, release automation, calibration, artifact verification, mock HIL, or real HIL workflows from the source tree.
---

# Flux Purr Developer Operations

Use this skill for repository-internal developer operations from the source tree. It is the developer-side counterpart to the installed or released user surface described by `skills/flux-purr-user-operations`.

Read `skills/flux-purr-developer-policy/SKILL.md` first for the repo-wide developer rules. This skill does not replace that policy layer or repo `AGENTS.md`; it only covers developer operations, hardware validation, and HIL boundaries.

## Default Tooling

- Build and test devd/CLI with `bun run check:devd` or `cargo test --manifest-path tools/flux-purr-devd/Cargo.toml`.
- Start the local daemon from this checkout, not from a global installation:

```bash
cargo run --manifest-path tools/flux-purr-devd/Cargo.toml --bin flux-purr-devd -- serve \
  --bind 127.0.0.1:<leased-devd-port> \
  --serial-port <owner-authorized-port> \
  --artifact-root <repo-root>
```

- Pass `--bind`, `--serial-port`, `--artifact-root`, `--allow-dev-cors`, and `--allow-real-flash` explicitly for test setups. Real flash remains disabled unless the owner explicitly authorizes it.
- Use released-style CLI paths even from source: `cargo run --manifest-path tools/flux-purr-devd/Cargo.toml --bin flux-purr -- ...`.
- Use `scripts/devd-hardware-smoke.py --device-id mock-fp-lab-01 --allow-mock-device` only for mock HTTP contract proof. Never report mock smoke as hardware validation.
- For Web live development, start Vite with an explicit `VITE_FLUX_PURR_DEVD_URL=http://127.0.0.1:<leased-devd-port>` and `VITE_FLUX_PURR_ENABLE_DEVD=1`; do not rely on the default devd port when a port lease is required.

## IsolaPurr Boundary

- If Flux Purr testing uses IsolaPurr as the external HUB, USB-C power path, or bench source, also read `$isolapurr-developer-operations`.
- IsolaPurr operations must stay on the IsolaPurr side of the boundary: controlling the HUB/source is allowed only as needed for the bench setup.
- Do not use IsolaPurr checkout commands, host tools, MCU selector, release assets, or installed binaries as substitutes for Flux Purr devd, CLI, firmware, Web, or HIL validation.
- Do not install or update global tools unless the owner explicitly authorizes that separate operation.

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
