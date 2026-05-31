import { defineConfig } from '@playwright/test'

const webPort = Number(process.env.E2E_WEB_PORT ?? 4173)
const devdPort = Number(process.env.E2E_DEVD_PORT ?? 30081)

export default defineConfig({
  testDir: './e2e',
  use: {
    baseURL: `http://127.0.0.1:${webPort}`,
  },
  webServer: {
    command: `VITE_FLUX_PURR_DEVD_URL=http://127.0.0.1:${devdPort} VITE_FLUX_PURR_ENABLE_DEVD=1 bun run dev --host 127.0.0.1 --port ${webPort}`,
    url: `http://127.0.0.1:${webPort}`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
})
