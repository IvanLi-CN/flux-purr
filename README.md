# Flux Purr

Flux Purr is a device mono-repo for an embedded firmware + React control console stack.

## Native devd

`tools/flux-purr-devd` is the localhost native daemon for browser-to-device workflows that cannot be handled safely by Web UI alone: USB/serial discovery, exclusive leases, bounded monitor events, WiFi provisioning bridge, firmware artifact dry-run, and guarded `espflash` execution.

The daemon is started with `flux-purr-devd serve`. Default bind is `127.0.0.1:30080`, and loopback binds enable development CORS for local `localhost` / loopback origins so the Vite console can reach the daemon from its own local port. Real flashing stays disabled unless `--allow-real-flash` or `FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1` is set; dry-run verification is available without hardware.

The user-facing command-line entry point is `flux-purr`. It talks to `flux-purr-devd`, creates and heartbeats device leases automatically, and covers `devices`, `status`, `runtime`, `wifi`, `flash`, `monitor`, `hardware`, and `usb-port` commands. User hardware memory and the default USB port live in the OS user config directory; `FLUX_PURR_HOME` overrides that location. `flux-purr usb-port set <port>` updates the remembered default for future daemon starts.

For repository-side developer work, start from `skills/flux-purr-developer-operations/SKILL.md`. It is the default source/devd/HIL/release entry for Flux Purr contributors and same-identity agents. If the task crosses into IsolaPurr bench, HUB, or source-side maintenance, switch to `$isolapurr-developer-operations` instead of treating IsolaPurr tooling as part of the default Flux Purr flow.

## Repository layout

- `firmware/` - Rust `no_std` firmware domain crate (ESP32-S3 first)
- `web/` - React + Vite + shadcn/ui + Storybook console
- `tools/flux-purr-devd/` - native `flux-purr` CLI and `flux-purr-devd` daemon
- `docs/specs/` - executable specs and acceptance contracts
- `docs/research/` - upstream research baselines for hardware/firmware derivative work
- `docs/hardware/` - board-level pin map and power-chain baselines
- `.github/` - CI, label gate, and release workflows
- `scripts/` - shared check scripts used by hooks and CI

## Quick start

```bash
# Install root tooling (lefthook + commitlint)
bun install

# Install web dependencies
bun install --cwd web

# Install git hooks
bun run hooks:install
```

## Local checks

```bash
bun run check:firmware:fmt
bun run check:firmware:clippy
bun run check:firmware:build
bun run check:web
bun run check:web:build
bun run check:storybook
```

## PR labels, releases, and branch protection

PRs targeting `main` must carry exactly one release type label and exactly one release channel label:

- Type: `type:patch`, `type:minor`, `type:major`, `type:docs`, or `type:skip`
- Channel: `channel:stable` or `channel:rc`

`type:patch`, `type:minor`, and `type:major` publish one product release after `CI Main` succeeds on the merged commit. `type:docs` and `type:skip` intentionally skip the release workflow. Stable releases use `vX.Y.Z`; RC releases use `vX.Y.Z-rc.<sha7>`.

`Label Gate` records the validated release intent against the PR head SHA before merge. After `CI Main` succeeds, release intent is frozen on `main` in git notes under `refs/notes/release-snapshots`; the product release job reads only that snapshot, not mutable post-merge PR labels. Manual release backfill uses the `Release Product` workflow with an explicit `main` commit SHA and reads the existing snapshot for that commit.

Each product release attaches Web, Firmware, host-tools, and `flux-purr-release-manifest-vX.Y.Z.json` assets. The manifest records per-component hashes, `contentSha256`, `sourceSha`, protocol versions, `changedSincePrevious`, and `updateReason`; users should update only components marked changed.

The branch protection contract is declared in [.github/quality-gates.json](.github/quality-gates.json). GitHub should protect `main`, require PRs, require signed commits, disallow force pushes/deletions, and require these checks before merge:

- `Validate PR labels`
- `Firmware checks`
- `Web checks`

## Firmware target notes

Current default target direction is ESP32-S3. For Xtensa builds in CI/release:

```bash
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --release
```

Current hardware baseline assumes `ESP32-S3FH4R2`; keep API contracts stable if the MCU selection changes again.

Current firmware runtime baseline also assumes:

- CH224Q default PD request is `20 V`
- optional firmware variants can switch the boot PD request to `12 V` or `28 V` via Cargo features
- heater control prefers `PPS/AVS + MOS static switching` only when CH224Q power data proves PPS covers `20 V`; otherwise it falls back to the original fixed-PD `GPIO47` PWM backend
- Dashboard center double toggles the active-cooling policy
- Dashboard fan line renders `OFF / AUTO / RUN`, while the real output contract remains `fanEnabled + fanPwmPermille`

PD request build variants:

```bash
# default runtime image (20 V)
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --release

# 12 V variant
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --no-default-features --features esp32s3,web_serial,pd-request-12v --bin flux-purr --release

# 28 V variant
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --no-default-features --features esp32s3,web_serial,pd-request-28v --bin flux-purr --release
```

Current hardware design notes and manufacturing support assets are frozen in:

- [docs/hardware/tps62933-dual-rail-power-design.md](docs/hardware/tps62933-dual-rail-power-design.md)
- [docs/hardware/heater-power-switch-design.md](docs/hardware/heater-power-switch-design.md)
- [docs/hardware/heater-plate-design.md](docs/hardware/heater-plate-design.md)
- [docs/hardware/heater-stack-support-7p0cm.md](docs/hardware/heater-stack-support-7p0cm.md)
- [docs/hardware/fan-pcb-variants.md](docs/hardware/fan-pcb-variants.md)
- [docs/hardware/enclosure-5p6cm.md](docs/hardware/enclosure-5p6cm.md)

The fan rail is maintained as two sibling PCB variants that keep the same firmware-facing GPIO and status contract:

- `fan-5v`: adjustable `3.0 V ~ 5.06 V`
- `fan-12v`: adjustable `6.6 V ~ 12.0 V`

## Research baseline

- PD mini hotplate derivative baseline:
  - [docs/research/mini-hotplate/README.md](docs/research/mini-hotplate/README.md)
