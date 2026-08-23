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
bun run build:firmware:web
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

`bun run build:firmware:web` builds the current ESP32-S3 release firmware and writes one
strictly validated `.fluxpurr-fw` package directly to
`firmware/target/flux-purr-web-artifacts/`. The Vite development server watches that
directory and serves the exact bytes through the same-origin firmware catalog without
creating a copy under `web/public`.

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

`deviceId` is the stable physical identity reported by the device, not a transport target ID, alias, address, or credential. `demo` and the Demo Inspector's `demoScene`, `demoLease`, `demoNetwork`, and `demoArtifact` are typed search parameters retained across navigation. `uiDemo` remains a production-only typed root-entry parameter for the LAN pairing demo. Collection and device index routes replace to their canonical leaf; an unknown identity keeps its URL and renders recovery actions.

`bun run build:demo` produces `dist-demo`, a public mock-only variant that always opens the full control console at `/devices/fp-lab-01/overview`. It ignores `demo=false`, stored live preference, and `uiDemo`; it never enables devd, Web Serial, direct LAN, or hardware writes. The Demo Inspector stores scene and fault state in the typed URL while its expanded/collapsed layout stays local.

Both EdgeOne variants use `public/edgeone.json` to rewrite `/devices` history-route requests to `/index.html` while serving static assets directly. The public Demo pipeline deploys the verified `web-demo-bundle` through `.github/workflows/deploy-edgeone-demo.yml` after a successful `main` push. It requires the restricted `EDGEONE_API_TOKEN` and `EDGEONE_DEMO_PROJECT_NAME` secrets; the `flux-purr-demo` project owns the `flux-purr-demo.ivanli.cc` domain binding and certificate.

The stable implementation surface is:

- `src/features/control-plane-demo/**` for runtime data, types, and UI components
- `src/features/control-plane-demo/live-devd.ts` for local devd discovery
- `src/routes/**` and generated `src/routeTree.gen.ts` for the production route manifest
- `src/routing/**` for typed search, console route state, redirects, and route preferences
