import http from 'node:http'
import { expect, test } from '@playwright/test'

const devdPort = Number(process.env.E2E_DEVD_PORT ?? 30081)
const devdBaseUrl = `http://127.0.0.1:${devdPort}`
const artifactPath = 'firmware/target/xtensa-esp32s3-none-elf/release/flux-purr'
const artifactSha = 'sha256:e2e'
const deviceId = 'serial-e2e'

test.describe('control plane live devd bridge', () => {
  let server: http.Server
  const requests: Array<{ method: string; path: string; body: unknown }> = []
  const sseClients = new Set<http.ServerResponse>()

  test.beforeAll(async () => {
    server = http.createServer(async (request, response) => {
      const method = request.method ?? 'GET'
      const url = new URL(request.url ?? '/', devdBaseUrl)
      const body = await readJsonBody(request)
      requests.push({ method, path: url.pathname, body })

      if (method === 'OPTIONS') {
        sendJson(response, 204, null)
        return
      }

      if (method === 'GET' && url.pathname === '/api/v1/devices') {
        sendJson(response, 200, {
          devices: [
            {
              id: 'mock-fp-lab-01',
              displayName: 'Daemon mock target',
              portPath: null,
              transport: 'mock',
              connection: 'connected',
              identity: {
                ...identity(['identity', 'status']),
                deviceId: 'mock-fp-lab-01',
                hostname: 'mock-fp-lab-01',
              },
              network: network('connected'),
              status: status(network('connected')),
              events: [],
            },
            {
              id: deviceId,
              displayName: 'E2E authorized USB target',
              portPath: '/dev/cu.usbmodem-e2e',
              transport: 'native_serial',
              connection: 'disconnected',
              identity: identity([
                'identity',
                'status',
                'network',
                'wifi_config',
                'monitor',
                'flash',
              ]),
              network: network('idle'),
              status: status(network('idle')),
              events: [
                {
                  id: 'event-e2e-flash',
                  timestamp: '1000',
                  deviceId,
                  kind: 'flash',
                  message: 'artifact dry-run passed',
                  payload: { artifactId: 'local-esp32s3-release', dryRun: true },
                },
              ],
            },
          ],
        })
        return
      }

      if (method === 'GET' && url.pathname === '/api/v1/artifacts') {
        sendJson(response, 200, {
          artifacts: [
            {
              artifactId: 'local-esp32s3-release',
              name: 'Local ESP32-S3 release',
              version: 'local-build',
              gitSha: 'e2e',
              buildId: 'e2e-build',
              targetChip: 'esp32s3',
              profile: 'release + web_serial',
              features: ['web_serial'],
              protocol: 'flux-purr.usb.v1',
              files: [
                {
                  kind: 'elf',
                  path: artifactPath,
                  sha256: artifactSha,
                  size: 964564,
                  flashAddress: null,
                },
              ],
            },
          ],
        })
        return
      }

      if (method === 'POST' && url.pathname === `/api/v1/devices/${deviceId}/leases`) {
        sendJson(response, 200, { leaseId: 'lease-e2e', deviceId, ttlMs: 8000 })
        return
      }

      if (method === 'POST' && url.pathname === '/api/v1/leases/lease-e2e/heartbeat') {
        sendJson(response, 200, { leaseId: 'lease-e2e', deviceId, ttlMs: 8000 })
        return
      }

      if (method === 'DELETE' && url.pathname === '/api/v1/leases/lease-e2e') {
        sendJson(response, 200, { released: true })
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/identity`) {
        sendJson(
          response,
          200,
          identity(['identity', 'status', 'network', 'usb_jsonl', 'wifi_config', 'monitor'])
        )
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/network`) {
        sendJson(response, 200, network('connected'))
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/status`) {
        sendJson(response, 200, status(network('connected')))
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/events`) {
        sendSse(response, sseClients, {
          id: 'event-e2e-serial-timeout',
          timestamp: '1001',
          deviceId,
          kind: 'serial',
          message: 'native serial RPC failed',
          payload: {
            stage: 'status',
            code: 'usb_response_timeout',
            retryable: true,
          },
        })
        return
      }

      if (method === 'PUT' && url.pathname === `/api/v1/devices/${deviceId}/wifi`) {
        sendJson(response, 200, {
          network: {
            state: bodyField(body, 'op') === 'clear' ? 'disabled' : 'connected',
            ssid: bodyField(body, 'op') === 'clear' ? null : bodyField(body, 'ssid'),
            ip: bodyField(body, 'op') === 'clear' ? null : '192.0.2.11',
            gateway: null,
            dns: [],
            wifiRssi: bodyField(body, 'op') === 'clear' ? null : -49,
            lastError: null,
          },
        })
        return
      }

      if (method === 'PUT' && url.pathname === `/api/v1/devices/${deviceId}/runtime`) {
        sendJson(response, 200, {
          ...status(network('connected')),
          targetTempC:
            typeof bodyField(body, 'targetTempC') === 'number'
              ? bodyField(body, 'targetTempC')
              : 220,
          activeCoolingEnabled:
            typeof bodyField(body, 'activeCoolingEnabled') === 'boolean'
              ? bodyField(body, 'activeCoolingEnabled')
              : true,
          fanDisplayState: bodyField(body, 'activeCoolingEnabled') === false ? 'OFF' : 'AUTO',
          heaterEnabled:
            typeof bodyField(body, 'heaterEnabled') === 'boolean'
              ? bodyField(body, 'heaterEnabled')
              : true,
          heaterOutputPercent: bodyField(body, 'heaterEnabled') === false ? 0 : 18,
        })
        return
      }

      if (method === 'POST' && url.pathname === '/api/v1/artifacts/verify') {
        sendJson(response, 200, {
          artifactId: 'local-esp32s3-release',
          verified: true,
          files: [{ kind: 'elf', sha256: artifactSha, size: 964564, ok: true }],
        })
        return
      }

      if (method === 'POST' && url.pathname === `/api/v1/devices/${deviceId}/flash`) {
        sendJson(response, 200, {
          artifactId: 'local-esp32s3-release',
          dryRun: true,
          status: 'passed',
          message: 'Artifact verified; no flash write performed.',
        })
        return
      }

      sendJson(response, 404, {
        error: { code: 'not_found', message: url.pathname, retryable: false },
      })
    })

    await new Promise<void>((resolve, reject) => {
      server.once('error', reject)
      server.listen(devdPort, '127.0.0.1', resolve)
    })
  })

  test.afterAll(async () => {
    for (const response of sseClients) {
      response.end()
    }
    sseClients.clear()
    await new Promise<void>((resolve, reject) => {
      server.close((error) => (error ? reject(error) : resolve()))
    })
  })

  test.beforeEach(() => {
    requests.length = 0
  })

  test('discovers live devd target and completes artifact dry-check through HTTP bridge', async ({
    page,
  }) => {
    await page.goto('/?demo=false')

    const targetRegion = page.getByRole('region', { name: '当前目标' })
    await expect(page.getByRole('combobox', { name: '目标设备' })).toContainText('/ DEVD')
    await expect(targetRegion).toContainText('传输')
    await expect(targetRegion).toContainText('DEVD')
    await expect(targetRegion).toContainText('租约')
    await expect(targetRegion).toContainText('有效')
    await page.waitForTimeout(1800)
    await expect(page.getByText('181.5').first()).toBeVisible()
    await expect(page.getByText('Heater 18%')).toBeVisible()
    await expect(
      page.getByText('native serial RPC failed: status / usb_response_timeout').first()
    ).toBeVisible()

    await page.getByRole('button', { name: /更新/i }).click()
    await expect(page.getByRole('combobox', { name: 'Firmware artifact' })).toContainText(
      'local-build'
    )
    await expect(page.getByText(`esp32s3 · flux-purr.usb.v1 · ${artifactSha}`)).toBeVisible()

    await page.getByRole('button', { name: 'Run dry-check' }).click()

    await expect(page.getByText('Dry-run passed', { exact: true })).toBeVisible()
    await expect(page.getByText('local-build verified 1 local file.')).toBeVisible()
    await expect
      .poll(
        () =>
          requests.filter(
            (request) => request.method === 'POST' && request.path === '/api/v1/artifacts/verify'
          ).length
      )
      .toBeGreaterThanOrEqual(1)
    await expect
      .poll(
        () =>
          requests.filter(
            (request) =>
              request.method === 'POST' && request.path === `/api/v1/devices/${deviceId}/flash`
          ).length
      )
      .toBeGreaterThanOrEqual(1)
    await expect
      .poll(
        () =>
          requests.filter(
            (request) =>
              request.method === 'GET' && request.path === `/api/v1/devices/${deviceId}/events`
          ).length
      )
      .toBeGreaterThanOrEqual(1)
  })

  test('sends runtime commands through the active devd lease', async ({ page }) => {
    await page.goto('/?demo=false')

    await expect(page.getByRole('combobox', { name: '目标设备' })).toContainText('/ DEVD')

    await page.getByRole('button', { name: /总览/i }).click()
    await page.getByLabel('Dashboard target temperature').fill('235')
    await expect(page.getByText('Target updated')).toBeVisible()

    await page.getByRole('button', { name: /设置/i }).click()
    await page.getByRole('button', { name: 'OFF' }).click()
    await expect(page.getByText('Fan policy updated', { exact: true })).toBeVisible()

    await page.getByRole('button', { name: /总览/i }).click()
    await page.getByRole('button', { name: 'Hold heater' }).click()
    await expect(page.getByText('Heater held')).toBeVisible()

    expect(wifiRequests()).toHaveLength(0)
    await expect.poll(() => runtimeRequests().length).toBeGreaterThanOrEqual(3)
    expect(runtimeRequests()).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          body: expect.objectContaining({ leaseId: 'lease-e2e', targetTempC: 235 }),
        }),
        expect.objectContaining({
          body: expect.objectContaining({
            leaseId: 'lease-e2e',
            activeCoolingEnabled: false,
          }),
        }),
        expect.objectContaining({
          body: expect.objectContaining({ leaseId: 'lease-e2e', heaterEnabled: false }),
        }),
      ])
    )
  })

  function wifiRequests() {
    return requests.filter(
      (request) => request.method === 'PUT' && request.path === `/api/v1/devices/${deviceId}/wifi`
    )
  }

  function runtimeRequests() {
    return requests.filter(
      (request) =>
        request.method === 'PUT' && request.path === `/api/v1/devices/${deviceId}/runtime`
    )
  }
})

async function readJsonBody(request: http.IncomingMessage) {
  const chunks: Buffer[] = []
  for await (const chunk of request) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk))
  }
  if (chunks.length === 0) {
    return null
  }

  return JSON.parse(Buffer.concat(chunks).toString('utf8'))
}

function bodyField(body: unknown, field: string) {
  return body && typeof body === 'object' && field in body
    ? (body as Record<string, unknown>)[field]
    : undefined
}

function sendJson(response: http.ServerResponse, statusCode: number, payload: unknown) {
  response.writeHead(statusCode, {
    'access-control-allow-headers': 'content-type',
    'access-control-allow-methods': 'GET,POST,PUT,DELETE,OPTIONS',
    'access-control-allow-origin': '*',
    'content-type': 'application/json',
  })
  response.end(payload === null ? '' : JSON.stringify(payload))
}

function sendSse(
  response: http.ServerResponse,
  clients: Set<http.ServerResponse>,
  event: Record<string, unknown>
) {
  response.writeHead(200, {
    'access-control-allow-origin': '*',
    'cache-control': 'no-cache',
    connection: 'keep-alive',
    'content-type': 'text/event-stream',
  })
  response.write(`event: ${event.kind}\n`)
  response.write(`data: ${JSON.stringify(event)}\n\n`)
  clients.add(response)
  response.on('close', () => clients.delete(response))
}

function identity(capabilities: string[]) {
  return {
    deviceId,
    firmwareVersion: 'fw/e2e',
    buildId: 'e2e-build',
    gitSha: 'e2e',
    board: 'esp32-s3',
    apiVersion: '2026-05-23',
    protocolVersion: 'flux-purr.usb.v1',
    hostname: 'flux-purr-e2e',
    capabilities,
  }
}

function network(state: 'idle' | 'connected') {
  return {
    state,
    ssid: state === 'connected' ? 'FluxPurr-E2E' : null,
    ip: state === 'connected' ? '192.0.2.10' : null,
    gateway: null,
    dns: [],
    wifiRssi: state === 'connected' ? -51 : null,
    lastError: null,
  }
}

function status(networkSummary: ReturnType<typeof network>) {
  return {
    mode: 'sampling',
    uptimeSeconds: 42,
    currentTempC: 181.5,
    targetTempC: 220,
    heaterEnabled: true,
    heaterOutputPercent: 18,
    activeCoolingEnabled: true,
    fanDisplayState: 'AUTO',
    fanEnabled: true,
    fanPwmPermille: 500,
    voltageMv: 20000,
    currentMa: 720,
    boardTempCenti: 3600,
    pdRequestMv: 20000,
    pdContractMv: 20000,
    pdState: 'ready',
    frontpanelKey: null,
    network: networkSummary,
  }
}
