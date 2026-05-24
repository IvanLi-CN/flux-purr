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
- `FrontPanelDisplay`
- `WifiConfigForm`
- `TelemetryTrendCard`

## Control plane app

The app entry renders the Flux Purr control console. It demonstrates Dashboard runtime control, Settings preset/fan policy editing, Update dry-check, a compact target dropdown, and a desktop global log panel.

The stable implementation surface is:

- `src/features/control-plane-demo/**` for runtime data, types, and UI components
- `src/features/control-plane-demo/live-devd.ts` for local devd discovery
