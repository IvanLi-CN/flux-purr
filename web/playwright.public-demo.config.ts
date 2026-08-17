import { defineConfig } from '@playwright/test'

const webPort = Number(process.env.E2E_WEB_PORT ?? 4173)
const canReuseExistingServer = !process.env.CI && process.env.E2E_REUSE_SERVER === '1'

export default defineConfig({
  testDir: './e2e',
  testMatch: 'public-demo.spec.ts',
  use: {
    baseURL: `http://127.0.0.1:${webPort}`,
  },
  webServer: {
    command: `bun run build:demo && bunx vite preview --outDir dist-demo --host 127.0.0.1 --port ${webPort} --strictPort`,
    url: `http://127.0.0.1:${webPort}`,
    reuseExistingServer: canReuseExistingServer,
    timeout: 120_000,
  },
})
