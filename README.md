# Flux Purr

Flux Purr is a device mono-repo for an embedded firmware + React control console stack.

## Native devd

`tools/flux-purr-devd` is the localhost native daemon for browser-to-device workflows that cannot be handled safely by Web UI alone: USB/serial discovery, exclusive leases, bounded monitor events, WiFi provisioning bridge, firmware artifact dry-run, and guarded `espflash` execution.

The daemon is started with `flux-purr-devd serve`. Default bind is `127.0.0.1:30080`, and loopback binds enable development CORS for local `localhost` / loopback origins so the Vite console can reach the daemon from its own local port. Real flashing stays disabled unless `--allow-real-flash` or `FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1` is set; dry-run verification is available without hardware. Before every real flash, devd stages and verifies the complete existing `flux_cfg` record; after the app write it restores that record at the target address and verifies it again. An unsafe or failed preflight blocks the app write.

The user-facing command-line entry point is `flux-purr`. It talks to `flux-purr-devd`, creates and heartbeats device leases automatically, and covers `devices`, `status`, `runtime`, `wifi`, `flash`, `monitor`, `hardware`, and `usb-port` commands. User hardware memory and the default USB port live in the OS user config directory; `FLUX_PURR_HOME` overrides that location. `flux-purr usb-port set <port>` updates the remembered default for future daemon starts.

## WiFi/LAN Control

The ESP32-S3 firmware exposes a trusted-LAN HTTP v1 control plane after USB provisioning WiFi credentials. It uses DHCP by default, advertises a MAC-derived hostname through DHCP and `_http._tcp.local` mDNS/DNS-SD, and allows USB/devd to configure static IPv4 when needed. Entering the front-panel WiFi Info page creates a four-digit pairing code; leaving the page invalidates it immediately. Chromium users can pair at `https://flux-purr.ivanli.cc` by entering an address or explicitly scanning a bounded private IPv4 CIDR through anonymous `/health` requests; this direct-browser scan never uses `devd`. Safari direct-LAN control is intentionally unsupported because it cannot meet the required private-network access flow.

The stable token is stored only on the device and the local client record, never in URLs or user-facing logs. LAN writes use an exclusive 30-second lease. USB/devd remains the only route for initial WiFi setup, firmware flash, and pairing-token reset. In the live Web console, choose `Add device`; its default, visibly selected `WiFi` option exposes the device's private HTTP address entry. Select Web Serial or Bridge to switch away from that connection method. Connect before the console requests any authorization. Every device exposes a low-frequency public identity summary first. The current `required` pairing policy then opens a four-digit-code dialog; future `optional` devices claim without a code, while `unavailable` devices remain at public basic information only. Chromium restores only previously paired local records and the last manually entered direct-LAN CIDR as a local form preference; neither action scans the network automatically. The CIDR controls and discovered results remain visible while the operator works. Safari direct-LAN control is unavailable. Native `devd` targets with `wifi_config` keep the WiFi section visible; it remains locked with an upgrade reason until `wifi_state_v2` and an active USB lease are present. Direct LAN and Web Serial targets cannot configure credentials. WiFi display follows only device-published versioned snapshots: a configuration runs at most three attempts in 30 seconds, recoverable attempts remain `connecting`, and exhaustion publishes one terminal `error` that cannot auto-recover under the same generation. A confirmed `NetworkSummary.ssid` remains visible after connection while only the password is cleared. See [the HTTP contract](docs/interfaces/http-api.md).

The production Web bundle is deployed to EdgeOne only after `CI Main` succeeds on `main`. CI uploads the verified `web/dist` artifact, and `.github/workflows/deploy-edgeone.yml` deploys that exact artifact. The repository requires restricted `EDGEONE_API_TOKEN` and `EDGEONE_PROJECT_NAME` secrets; the `flux-purr.ivanli.cc` custom-domain binding remains EdgeOne project configuration. CI also builds a separate `web/dist-demo` mock-only artifact and `.github/workflows/deploy-edgeone-demo.yml` deploys it to the `flux-purr-demo` Makers project with `EDGEONE_DEMO_PROJECT_NAME`; `flux-purr-demo.ivanli.cc` is never a live direct-LAN origin.

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

Development builds derive `nextPatch(VERSION)-dev.<short-sha>`. With `VERSION=0.22.0` and source commit `abcdef0...`, every local consumer reports `0.22.1-dev.abcdef0`; no development command writes `VERSION`.

Every source commit that passes `CI Main` is released separately. The release controller creates one child Release Commit that changes only `VERSION`, stores the source SHA and product version in commit trailers, builds and verifies all assets from that commit, creates `vX.Y.Z` (or `vX.Y.Z-rc.N`), publishes the manifest, and only then fast-forwards `main`. A failed run keeps the immutable candidate at `release/product-main`; `operation=recover` reuses that same Release Commit. `operation=exact` is for major, minor, or RC values, and `operation=promote` creates a new stable Release Commit from a completed RC.

PRs must carry exactly one `type:patch|minor|major|docs|skip` label and exactly one `channel:stable|rc` label. The trusted `Label Gate` validates those labels and freezes the intent on the PR head; the release workflow later consumes the mainline snapshot. Labels select release intent and channel, while the numeric version still comes only from `VERSION`.

The branch protection contract is declared in [.github/quality-gates.json](.github/quality-gates.json). GitHub should protect `main`, require signed commits and the `Validate PR labels`, `Release completion`, `Firmware checks`, `DEVD checks`, `Web checks`, and `Worktree bootstrap` checks, and allow only the dedicated release App to bypass the protected Release Commit fast-forward.

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
