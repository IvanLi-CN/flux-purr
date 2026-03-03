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

## Firmware target notes

Current default target direction is ESP32-S3. For Xtensa builds in CI/release:

```bash
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --release
```

If hardware selection changes to ESP32-C3, keep API contracts stable and switch target in workflows.

## Research baseline

- PD mini hotplate derivative baseline:
  - [docs/research/mini-hotplate/README.md](docs/research/mini-hotplate/README.md)
