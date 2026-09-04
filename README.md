# Flux Purr

Flux Purr is a device mono-repo for an embedded firmware + React control console stack.

## Native devd

`tools/flux-purr-devd` is the local daemon for browser-to-device workflows that cannot be handled safely by Web UI alone: USB/serial discovery, exclusive leases, bounded monitor events, and WiFi provisioning bridge. Its HTTP boundary is for the Web surface only; the `flux-purr` CLI never communicates with devd through HTTP.

Firmware operations have two user classes and two firmware sources:

| User class | Command | Firmware source |
| --- | --- | --- |
| General User | `flux-purr update --port <serial-port> --bundle <local.fluxpurr-fw>` | Locally available, product-signed `.fluxpurr-fw` bundle |
| Developer | `flux-purr flash --port <serial-port> [--elf <local-elf>]` | Local ELF; defaults to `firmware/target/xtensa-esp32s3-none-elf/release/flux-purr` |
| Developer | `flux-purr recover --port <serial-port> --elf <local-elf> --confirm ERASE` | Local ELF only |

Every firmware operation requires the exact serial port. `update` starts a managed local devd when no `--devd <local-control-socket>` is supplied; it neither selects nor remembers a port. `flash` uses no devd, URL, HTTP, bundle, artifact ID, or manifest. It automatically creates an encrypted EEPROM archive before writing unless the Developer explicitly confirms the emergency bypass. `recover` is the explicit MCU-internal-Flash erase path and does not touch EEPROM. The complete contract is [Firmware Update And Developer Flash](docs/specs/firmware-update-and-developer-flash/SPEC.md); its [implementation status](docs/specs/firmware-update-and-developer-flash/IMPLEMENTATION.md) records the current gaps.

For source workflows, the project Justfile keeps common host commands short: `just cli <arguments>` runs the host CLI, `just buzzer-play --device <device-id> [--devd <url>]` or `just buzzer-play --hardware <hardware-id> [--devd <url>]` opens the feature-gated interactive buzzer test session for an explicitly selected local target, and `just check-devd` runs its host validation. A real terminal keeps the session on the main screen so its text remains selectable and copyable; arrow keys and Home/End move the selection, `C` or `L` starts continuous playback, Enter or Space plays once (or stops a running loop), `S` stops, `R` refreshes, `M` toggles pointer capture, and `Q` or Escape exits. Use `--pointer` to start with pointer capture enabled; press `M` to release it before selecting/copying terminal text. The session exposes every production cue and the fixed feedback-arbitration scenarios. Bind a logical target only with the explicit `just hardware-save <hardware-id> <device-id> <devd-url>` command. The Justfile never scans for, aliases, or chooses a physical device.

## WiFi/LAN Control

The ESP32-S3 firmware exposes a trusted-LAN HTTP v1 control plane after USB provisioning WiFi credentials. It uses DHCP by default, advertises a MAC-derived hostname through DHCP and `_http._tcp.local` mDNS/DNS-SD, and allows USB/devd to configure static IPv4 when needed. Entering the front-panel WiFi Info page creates a four-digit pairing code; leaving the page invalidates it immediately. Chromium users can pair at `https://flux-purr.ivanli.cc` by entering an address or explicitly scanning a bounded private IPv4 CIDR through anonymous `/health` requests; this direct-browser scan never uses `devd`. Safari direct-LAN control is intentionally unsupported because it cannot meet the required private-network access flow.

The stable token is stored only on the device and the local client record, never in URLs or user-facing logs. LAN writes use an exclusive 30-second lease. USB configuration transports (Browser Web Serial or native `devd`) remain the only routes for initial WiFi setup, firmware flash, and pairing-token reset. In the live Web console, choose `Add device`; its default, visibly selected `WiFi` option exposes the device's private HTTP address entry. Select Web Serial or Bridge to switch away from that connection method. Connect before the console requests any authorization. Every device exposes a low-frequency public identity summary first. The current `required` pairing policy then opens a four-digit-code dialog; future `optional` devices claim without a code, while `unavailable` devices remain at public basic information only. Chromium restores only previously paired local records and the last manually entered direct-LAN CIDR as a local form preference; neither action scans the network automatically. The CIDR controls and discovered results remain visible while the operator works. Safari direct-LAN control is unavailable. WiFi settings are transport-scoped: an active native `devd` USB lease or a connected Web Serial target with `wifi_config` and `wifi_state_v2` may write credentials; direct LAN and `devd` WiFi/LAN bridge targets remain read-only and show an explanation. WiFi display follows only device-published versioned snapshots: a configuration runs at most three attempts in 30 seconds, recoverable attempts remain `connecting`, and exhaustion publishes one terminal `error` that cannot auto-recover under the same generation. After 10 seconds the Web form may send the USB-only `cancel` command, which succeeds only after the Device has stopped its WiFi station and published `idle`; this preserves stored credentials and never substitutes a host-side wait cancellation. Successful EEPROM WiFi persistence is page-bounded and continues to service PD on shared-bus hardware, so a credential change does not interrupt the power-control loop. A live reconfiguration bounds both station disconnect and controller stop before applying credentials and starting the station again; a timeout settles that generation as an error instead of panicking or rebooting the device. A confirmed `NetworkSummary.ssid` remains visible after connection while only the password is cleared. See [the HTTP contract](docs/interfaces/http-api.md).

The product Web and public-demo bundles are deployed to EdgeOne only by `Release Product`, after it has published and verified the versioned release assets. This avoids an independent deployment from `CI Main` and lets recovery reuse the same immutable archives. The repository requires restricted `EDGEONE_API_TOKEN`, `EDGEONE_PROJECT_NAME`, and `EDGEONE_DEMO_PROJECT_NAME` secrets; `flux-purr.ivanli.cc` and `flux-purr-demo.ivanli.cc` remain their respective EdgeOne project bindings, and the public demo is never a live direct-LAN origin.

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
# Seed the main checkout once
bun run bootstrap:dev
```

The first seeded checkout installs shared Git hooks and records the main worktree for later linked worktree bootstrap. After that, new linked worktrees automatically attempt the same repo-managed bootstrap during their first `post-checkout`.

Automatic bootstrap is intentionally warning-only:

- It installs repo-managed development dependencies in the current checkout: root `bun install --frozen-lockfile`, `web/` `bun install --frozen-lockfile`, Cargo fetch prewarm, and shared hooks refresh.
- Cargo fetch prewarm stays best-effort. It runs against a temporary workspace snapshot so the real checkout does not gain a bootstrap-generated `Cargo.lock`; if Cargo networking or the Xtensa toolchain is unhealthy, bootstrap prints a repair hint and continues.
- It does not install or modify system prerequisites such as Bun, Rust/rustup, `cargo +esp`, `jq`, or Playwright browsers.
- When a system prerequisite is missing, checkout still succeeds and the bootstrap output prints the exact recovery command.

Manual recovery remains available from any checkout:

```bash
bun run bootstrap:dev
bun run worktree:setup
```

## Developer guidance

- Repo-local developer policy: [docs/guides/flux-purr-developer-policy.md](docs/guides/flux-purr-developer-policy.md)
- Agent routing and non-bypassable hard gates: [AGENTS.md](AGENTS.md)

## Local checks

```bash
bun run check:firmware:fmt
bun run check:firmware:clippy
bun run check:firmware:build
bun run build:firmware:web
bun run check:web
bun run check:web:build
bun run check:storybook
bun run test:worktree-bootstrap
```

## Product version and releases

The root [`VERSION`](VERSION) file is the only product-version source. It contains one stable SemVer or `-rc.N` value and is read without modification by local development, tests, firmware, `flux-purr-devd`, the CLI, Web builds, bundles, manifests, and releases. Cargo and NPM package versions remain package metadata only.

Development builds derive `nextPatch(VERSION)-dev.<short-sha>`. With `VERSION=0.23.0` and source commit `abcdef0...`, every local consumer reports `0.23.1-dev.abcdef0`; no development command writes `VERSION`.

Every product PR is released separately. After its full PR checks pass, `Prepare product version` appends one VERSION-only preparation commit to that same PR branch. Its trailers bind the verified source SHA, exact product version, and validated label intent. `Release completion` rechecks the current labels, base, preparation diff, and the source commit's full CI before the PR enters `main` through the normal protected merge route; no release workflow writes `main` directly. `Release Product` builds, tags, publishes, and verifies that merged `main` commit. A failed publication leaves the committed `VERSION` locked on `main`; `operation=recover` may only republish that same merged commit and version.

PRs must carry exactly one `type:patch|minor|major|docs|skip` label and exactly one `channel:stable|rc` label. `Validate PR labels` remains required. Once that check and PR CI pass, the preparation workflow copies the validated intent into the VERSION-only commit. `type:patch + channel:stable` writes `nextPatch(VERSION)` automatically. `type:minor`, `type:major`, and `channel:rc` require a controlled exact preparation for the existing PR. Labels never calculate or override a product version.

The branch protection contract is declared in [.github/quality-gates.json](.github/quality-gates.json). GitHub should protect `main`, require signed commits and the `Validate PR labels`, `Release completion`, `Firmware checks`, `DEVD checks`, `Web checks`, and `Worktree bootstrap` checks. Product PRs must use a merge commit so the protected merge preserves the prepared commit as provenance. The existing workflow `GITHUB_TOKEN` writes only the already-open PR branch plus release tags/assets; it has no bypass role. Version preparation uses GitHub's `createCommitOnBranch` mutation, which produces a GitHub-signed and verified commit without a repository signing key, secret, variable, App, or GitHub Environment. Candidate tags are reserved before preparation and checked again before build; the preserved historical `v0.23.0` is audit-only, so the first complete new-chain release is `v0.23.1`.

Each product release attaches Web, firmware, host-tools, and `flux-purr-release-manifest-vX.Y.Z.json` assets. The manifest records per-component hashes, `contentSha256`, `sourceSha`, protocol versions, `changedSincePrevious`, and `updateReason`; users should update only components marked changed.

## Firmware target notes

Current default target direction is ESP32-S3. For Xtensa builds in CI/release:

```bash
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --target-dir firmware/target --release
```

Current hardware baseline assumes `ESP32-S3FH4R2`; keep API contracts stable if the MCU selection changes again.

Current firmware runtime baseline also assumes:

- the archived CH224Q board defaults to a `20 V` PD request and retains its existing high-voltage behavior
- the FUSB302BMPX board uses read-only `0x9x` identity selection at its colliding `0x22` address, has `GPIO7` PD interrupt wiring, and selects PPS APDOs from `5V` through `21V`, with fixed PDO fallback
- `>=20 V @ >=3 A` is the performance-guaranteed PD tier; lower accepted contracts are degraded operation and cannot run calibration
- contractual `3 A`/`5 A` limits bound software heater power (`60 W`/`100 W` at `20 V`) but are not measured VBUS current or physical OCP
- optional firmware variants can switch the boot PD request to `12 V` or `28 V` via Cargo features
- heater control uses the selected controller's supported path: CH224Q can use PPS/AVS, while FUSB302BMPX uses `5V..21V` PPS with fixed-PDO fallback and the `GPIO47` PWM backend
- Dashboard center double toggles the active-cooling policy
- Dashboard fan line renders `OFF / AUTO / RUN`, while the real output contract remains `fanEnabled + fanPwmPermille`

PD request build variants:

```bash
# default runtime image (20 V)
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --target-dir firmware/target --release

# 12 V variant
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --target-dir firmware/target --no-default-features --features esp32s3,web_serial,net_http,pd-request-12v --bin flux-purr --release

# 28 V variant
cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --target-dir firmware/target --no-default-features --features esp32s3,web_serial,net_http,pd-request-28v --bin flux-purr --release
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
