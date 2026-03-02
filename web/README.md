# Flux Purr Web

React control console for Flux Purr firmware.

## Stack

- React 19 + TypeScript + Vite
- Bun package/runtime tooling
- Biome for formatting + lint checks
- shadcn/ui component primitives
- Storybook for component contracts
- Playwright for e2e smoke checks

## Local commands

```bash
bun install --cwd web
bun run --cwd web dev
bun run --cwd web check
bun run --cwd web typecheck
bun run --cwd web build
bun run --cwd web storybook
bun run --cwd web build-storybook
```

## Stories included

- `ConsoleLayout`
- `DeviceStatusCard`
- `WifiConfigForm`
- `TelemetryTrendCard`
