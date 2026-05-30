---
name: flux-purr-user-operations
description: Operate released Flux Purr hardware as an end user. Use when Codex needs to guide or execute ordinary hardware operations through released flux-purr CLI/devd, browser Web Serial, USB port memory, firmware update checks, status/runtime/WiFi commands, or release-manifest-based upgrade decisions.
---

# Flux Purr User Operations

Use this skill for owner-facing hardware operations that should rely on released product tools, not source-only developer shortcuts.

## Control Surfaces

- Prefer the released `flux-purr` CLI for command-line hardware work. The CLI talks to local `flux-purr-devd`, creates and heartbeats leases automatically, and releases leases after each operation.
- Prefer browser Web Serial for browser-owned hardware access. Keep Web Serial as the official browser path.
- Use `flux-purr-devd serve` only as the local hardware owner behind CLI/Web privileged flows. It defaults to `127.0.0.1:30080`.
- Do not tell ordinary users to run `mcu-agentd`, `espflash`, `esptool`, or source-tree smoke scripts.

## User Hardware Memory

- Use `flux-purr identity|status --device <id>` or `--hardware <saved-id>` for readback proof.
- Use `flux-purr hardware available|recent|list|save|forget|path` for remembered user hardware.
- Use `flux-purr usb-port show|set <port>` for the default USB serial port.
- Treat `FLUX_PURR_HOME` as the override for Flux Purr user config; otherwise use the OS user config directory.
- State that a running `flux-purr-devd` must be restarted after `usb-port set` before the daemon picks up the new default.
- Remember only current USB targets as supported hardware. Do not present LAN HTTP, mDNS, or desktop app discovery as implemented.

## Updates

- Treat a product release tag `vX.Y.Z` as the unit of release.
- Check `flux-purr-release-manifest-vX.Y.Z.json` before recommending updates. Component update need is decided by `contentSha256`, `changedSincePrevious`, and `updateReason`.
- Avoid telling the user to upgrade unchanged components. Web, firmware, and host-tools can have different change decisions inside one product release.

## Safety

- Never perform flash, reset, serial write, WiFi write, or runtime mutation without an explicit target device or saved hardware id.
- For real flashing, require `flux-purr flash --no-dry-run --confirm FLASH` and a `flux-purr-devd serve --allow-real-flash` daemon.
- Redact WiFi passwords and secrets from summaries, logs, issue comments, and PR bodies.
