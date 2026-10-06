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

- Pass `--bind`, `--serial-port`, `--artifact-root`, and `--allow-dev-cors` explicitly for devd test setups. Developer `flash` and `recover` are direct serial/ROM commands and do not start or connect devd. Real flash remains disabled unless the owner explicitly authorizes it.
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
- Verify devd-backed `identity`/`status`, runtime write/readback/restore, and artifact behavior separately from Developer flash. `update`, `flash`, `recover`, and their devd equivalents retain the artifact, exact-port, identity, ROM/security, and EEPROM backup boundaries; they do not automatically invoke the device `flash_preparation` capability. The explicit device `prepare_flash`, `get_flash_preparation`, and `cancel_flash_preparation` operations are a separate USB protocol surface, not a host CLI command or a host timeout gate. Use `FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1 cargo run --manifest-path tools/flux-purr-devd/Cargo.toml --bin flux-purr -- flash --port <owner-authorized-port> [--elf <local-elf>]` for the normal direct flash path; it archives EEPROM before ROM write when the normal backup path is selected.
- Missing or unsupported device-side `flash_preparation` does not make a host flash incompatible and does not create an `application_unresponsive` bypass. With explicit owner authorization, the paired `--skip-backup --confirm NO_EEPROM_BACKUP` flags still skip only ROM probing and EEPROM snapshot/archive work.
- With explicit owner authorization, a Developer may bypass backup on only that exact port with `FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1 cargo run --manifest-path tools/flux-purr-devd/Cargo.toml --bin flux-purr -- flash --port <owner-authorized-port> [--elf <local-elf>] --skip-backup --confirm NO_EEPROM_BACKUP`. The paired bypass skips ROM probing and EEPROM snapshot/archive creation regardless of application or ROM state, including when old application firmware lacks the snapshot protocol. State in the HIL result that EEPROM health is unknown and no developer backup archive exists.
- Without the paired bypass, every snapshot, permission, durability, or verification failure remains blocking and requires its reported condition to be resolved before a normal flash. Developer archives are private raw `8192`-byte `.bin` files; legacy `.fpbk` files are directly deleted without being read or migrated, and raw bytes or digests never enter observable output.
- `flash` and `recover` preserve both stdout and stderr from each espflash invocation and report the observed phases, final phase, diagnosis category, exit code, and bounded output. A `finalize`/`FlashEnd` failure means the image may have been written but completeness and boot success are unconfirmed; do not report it as a complete flash.
- Do not use `mcu-agentd` as the acceptance path for CLI/devd HIL unless the owner explicitly changes the plan.
- Fixed idle/Flash Preparation product HIL must use the Device's original VIN calibration. Do not write temporary VIN fits or use VIN tolerance/dwell to establish a PD contract or readiness. Observe protocol-confirmed Fixed state and applied outputs, correlate them with the external source's live protocol/VBUS telemetry, and assert the final preferred-Fixed state after runtime restoration. Source telemetry remains empirical evidence, not an added host flash gate; successful writing alone does not establish idle-5V acceptance.

## Release Work

- Use product tag `vX.Y.Z`; do not recreate `web/v...` or `fw/v...` workflows.
- Publish Web, firmware, host-tools, and `flux-purr-release-manifest-vX.Y.Z.json` on the same GitHub Release.
- Keep release manifest components explicit with `sha256`, `contentSha256`, `sourceSha`, `protocolVersions`, `changedSincePrevious`, and `updateReason`.
- PRs that touch hardware behavior, release policy, CLI/devd contracts, or user operations must update relevant specs, solutions, and project docs.
