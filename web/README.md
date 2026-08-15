# Flux Purr Web

React control console for Flux Purr firmware.

## Stack

- React 19 + TypeScript + Vite
- TanStack Router with generated file-based routes
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
bun run --cwd web test:unit
bun run --cwd web test:storybook
bun run check:e2e
```

## Stories included

- `ConsoleLayout`
- `DeviceStatusCard`
- `FrontPanelDisplay`
- `WifiConfigForm`
- `TelemetryTrendCard`

## Control plane app

The app entry renders the Flux Purr control console. Production navigation is URL-controlled, while Storybook can render the same component without a router adapter.

Canonical routes:

- `/devices/new`
- `/devices/:deviceId/overview`
- `/devices/:deviceId/settings`
- `/devices/:deviceId/update`
- `/devices/:deviceId/calibration/heater-curve`
- `/devices/:deviceId/calibration/rtd-adc`
- `/devices/:deviceId/calibration/vin-adc`

`deviceId` is the stable physical identity reported by the device, not a transport target ID, alias, address, or credential. `demo` and `uiDemo` are typed search parameters retained across navigation. Collection and device index routes replace to their canonical leaf; an unknown identity keeps its URL and renders recovery actions.

The production EdgeOne deployment uses `public/edgeone.json` to rewrite history-route requests to `/index.html`.

The stable implementation surface is:

- `src/features/control-plane-demo/**` for runtime data, types, and UI components
- `src/features/control-plane-demo/live-devd.ts` for local devd discovery
- `src/routes/**` and generated `src/routeTree.gen.ts` for the production route manifest
- `src/routing/**` for typed search, console route state, redirects, and route preferences
