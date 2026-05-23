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
- `ControlPlaneDemo`
- `DeviceStatusCard`
- `FrontPanelDisplay`
- `WifiConfigForm`
- `TelemetryTrendCard`

## Control plane demo

The app entry renders a pure Web mock thermal bench console for the Flux Purr control-plane architecture. It demonstrates Dashboard runtime control, Settings preset/fan policy editing, Update dry-check, a compact target dropdown, and a desktop global log panel without connecting to real hardware.

The stable implementation surface is:

- `src/features/control-plane-demo/**` for mock data, types, and UI components
- `src/stories/ControlPlaneDemo.stories.tsx` for Storybook gallery, degraded state, mobile review, and interaction smoke coverage

The demo intentionally does not implement native USB daemon endpoints, USB CDC, WiFi HTTP, persistent storage, or real firmware flashing.
