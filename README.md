# Flux Purr

Flux Purr is a device mono-repo for an embedded firmware + React control console stack.

## Repository layout

- `firmware/` - Rust `no_std` firmware domain crate (ESP32-S3 first)
- `web/` - React + Vite + shadcn/ui + Storybook console
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

`type:patch`, `type:minor`, and `type:major` publish both Web and Firmware after `CI Main` succeeds on the merged commit. `type:docs` and `type:skip` intentionally skip both release workflows. Stable releases use `web/vX.Y.Z` and `fw/vX.Y.Z`; RC releases use `web/vX.Y.Z-rc.<sha7>` and `fw/vX.Y.Z-rc.<sha7>`.

`Label Gate` records the validated release intent against the PR head SHA before merge. After `CI Main` succeeds, release intent is frozen on `main` in git notes under `refs/notes/release-snapshots`; main release jobs read only that snapshot, not mutable post-merge PR labels. Manual release backfill uses the `Release Web` or `Release Firmware` workflow with an explicit `main` commit SHA and reads the existing snapshot for that commit.

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
- Dashboard center double toggles the active-cooling policy
- Dashboard fan line renders `OFF / AUTO / RUN`, while the real output contract remains `fanEnabled + fanPwmPermille`

PD request build variants:

```bash
# default runtime image (20 V)
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --release

# 12 V variant
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --no-default-features --features esp32s3,pd-request-12v --bin flux-purr --release

# 28 V variant
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --no-default-features --features esp32s3,pd-request-28v --bin flux-purr --release
```

Power design notes for the current board revision are frozen in:

- [docs/hardware/tps62933-dual-rail-power-design.md](docs/hardware/tps62933-dual-rail-power-design.md)
- [docs/hardware/heater-power-switch-design.md](docs/hardware/heater-power-switch-design.md)
- [docs/hardware/fan-pcb-variants.md](docs/hardware/fan-pcb-variants.md)

The fan rail is maintained as two sibling PCB variants that keep the same firmware-facing GPIO and status contract:

- `fan-5v`: adjustable `3.0 V ~ 5.06 V`
- `fan-12v`: adjustable `6.6 V ~ 12.0 V`

## Research baseline

- PD mini hotplate derivative baseline:
  - [docs/research/mini-hotplate/README.md](docs/research/mini-hotplate/README.md)
