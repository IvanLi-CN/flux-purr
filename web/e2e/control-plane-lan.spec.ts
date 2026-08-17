import { expect, type Page, test } from '@playwright/test'

const deviceUrl = 'http://192.168.1.18'
const deviceId = '001122334455'
const token = 'a'.repeat(64)

type RequestRecord = {
  method: string
  path: string
  headers: Record<string, string>
  body: unknown
}

test.use({ permissions: ['local-network-access'] })

test.describe('control plane direct LAN', () => {
  const requests: RequestRecord[] = []
  let targetTempC = 120
  let rejectBearerRequests = false
  let rejectLease = false
  let rejectHeartbeat = false
  let leaseTtlMs = 30_000
  let pairingFailureCode: 'pairing_code_invalid' | 'pairing_locked' | null = null
  let pairingMode: 'required' | 'optional' | 'unavailable' = 'required'
  let pairingActive = true
  let controlRevision = 7
  let rejectNextRuntimeAsStale = false
  let rejectHealth = false

  test.beforeEach(async ({ page }) => {
    requests.length = 0
    targetTempC = 120
    rejectBearerRequests = false
    rejectLease = false
    rejectHeartbeat = false
    leaseTtlMs = 30_000
    pairingFailureCode = null
    pairingMode = 'required'
    pairingActive = true
    controlRevision = 7
    rejectNextRuntimeAsStale = false
    rejectHealth = false
    await page.route(`${deviceUrl}/**`, async (route) => {
      const request = route.request()
      const url = new URL(request.url())
      const headers = request.headers()
      const body = request.postDataJSON() ?? null
      const requestOrigin = headers.origin ?? 'http://127.0.0.1:4173'
      requests.push({ method: request.method(), path: url.pathname, headers, body })

      if (request.method() === 'OPTIONS') {
        await route.fulfill({ status: 204, headers: corsHeaders(requestOrigin), body: '' })
        return
      }

      if (url.pathname === '/health' && request.method() === 'GET') {
        if (rejectHealth) {
          await route.fulfill({
            status: 503,
            headers: jsonHeaders(requestOrigin),
            body: JSON.stringify({
              error: { code: 'device_unavailable', message: 'Device is unavailable.' },
            }),
          })
          return
        }
        await route.fulfill({
          status: 200,
          headers: jsonHeaders(requestOrigin),
          body: JSON.stringify({
            ok: true,
            api: 'v1',
            deviceId,
            hostname: 'flux-purr-001122334455',
            firmwareVersion: 'fw/e2e',
            pairing: {
              mode: pairingMode,
              active: pairingMode === 'required' && pairingActive,
              attemptsRemaining: pairingMode === 'unavailable' ? 0 : 5,
            },
          }),
        })
        return
      }

      if (url.pathname === '/api/v1/pairing/claim' && request.method() === 'POST') {
        if (pairingMode === 'unavailable') {
          await route.fulfill({
            status: 403,
            headers: jsonHeaders(requestOrigin),
            body: JSON.stringify({
              error: { code: 'pairing_unavailable', message: 'Pairing is unavailable.' },
            }),
          })
          return
        }
        if (pairingFailureCode) {
          await route.fulfill({
            status: 409,
            headers: jsonHeaders(requestOrigin),
            body: JSON.stringify({
              error: { code: pairingFailureCode, message: 'Pairing was rejected.' },
            }),
          })
          return
        }
        await route.fulfill({
          status: 200,
          headers: jsonHeaders(requestOrigin),
          body: JSON.stringify({ token, deviceId, hostname: 'flux-purr-001122334455' }),
        })
        return
      }

      if (rejectBearerRequests || !headers.authorization) {
        await route.fulfill({
          status: 401,
          headers: jsonHeaders(requestOrigin),
          body: JSON.stringify({
            error: { code: 'unauthorized', message: 'Bearer token required.' },
          }),
        })
        return
      }

      if (url.pathname === '/api/v1/identity') {
        await route.fulfill({
          status: 200,
          headers: jsonHeaders(requestOrigin),
          body: JSON.stringify(identity()),
        })
        return
      }
      if (url.pathname === '/api/v1/network') {
        await route.fulfill({
          status: 200,
          headers: jsonHeaders(requestOrigin),
          body: JSON.stringify(network()),
        })
        return
      }
      if (url.pathname === '/api/v1/status') {
        await route.fulfill({
          status: 200,
          headers: jsonHeaders(requestOrigin, controlRevision),
          body: JSON.stringify(status(targetTempC)),
        })
        return
      }
      if (url.pathname === '/api/v1/leases' && request.method() === 'POST') {
        if (rejectLease) {
          await route.fulfill({
            status: 409,
            headers: jsonHeaders(requestOrigin),
            body: JSON.stringify({
              error: {
                code: 'lease_conflict',
                message: 'Another client owns the LAN control lease.',
              },
            }),
          })
          return
        }
        await route.fulfill({
          status: 200,
          headers: jsonHeaders(requestOrigin),
          body: JSON.stringify({ leaseId: 'lan-lease-e2e', ttlMs: leaseTtlMs }),
        })
        return
      }
      if (url.pathname === '/api/v1/leases' && request.method() === 'PUT') {
        if (rejectHeartbeat) {
          await route.fulfill({
            status: 409,
            headers: jsonHeaders(requestOrigin),
            body: JSON.stringify({
              error: { code: 'lease_expired', message: 'The LAN control lease expired.' },
            }),
          })
          return
        }
        await route.fulfill({
          status: 200,
          headers: jsonHeaders(requestOrigin),
          body: JSON.stringify({ leaseId: 'lan-lease-e2e', ttlMs: leaseTtlMs }),
        })
        return
      }
      if (url.pathname === '/api/v1/leases' && request.method() === 'DELETE') {
        await route.fulfill({
          status: 200,
          headers: jsonHeaders(requestOrigin),
          body: JSON.stringify({ released: true }),
        })
        return
      }
      if (url.pathname === '/api/v1/runtime' && request.method() === 'PUT') {
        if (rejectNextRuntimeAsStale) {
          rejectNextRuntimeAsStale = false
          controlRevision += 1
          targetTempC = 150
          await route.fulfill({
            status: 409,
            headers: jsonHeaders(requestOrigin, controlRevision),
            body: JSON.stringify({
              error: {
                code: 'stale_write',
                message: 'The control state changed after this client last read it.',
              },
            }),
          })
          return
        }
        if (typeof body === 'object' && body && 'targetTempC' in body) {
          targetTempC = Number(body.targetTempC)
        }
        controlRevision += 1
        await route.fulfill({
          status: 200,
          headers: jsonHeaders(requestOrigin, controlRevision),
          body: JSON.stringify(status(targetTempC)),
        })
        return
      }
      if (url.pathname === '/api/v1/events') {
        await route.fulfill({
          status: 200,
          headers: { ...corsHeaders(requestOrigin), 'content-type': 'text/event-stream' },
          body: 'data: {"kind":"status"}\n\n',
        })
        return
      }

      await route.fulfill({
        status: 404,
        headers: jsonHeaders(requestOrigin),
        body: JSON.stringify({ error: { code: 'not_found', message: 'Fixture route missing.' } }),
      })
    })
  })

  test('connects before it asks for a code, then pairs a direct LAN device and sends runtime control with a lease', async ({
    page,
  }) => {
    await page.goto('/?demo=false')
    await openLanPairing(page)

    await page.getByLabel('设备地址').fill(deviceUrl)
    await expect(page.getByLabel('四位配对码')).toHaveCount(0)
    await page.getByRole('button', { name: '连接设备' }).click()
    await expect(page.getByRole('dialog', { name: '输入 LAN 配对码' })).toBeVisible()
    expect(requests.filter((request) => request.path === '/health')).toHaveLength(1)
    expect(requests.filter((request) => request.path === '/api/v1/pairing/claim')).toHaveLength(0)
    const pairingDialog = page.getByRole('dialog', { name: '输入 LAN 配对码' })
    const deviceDetails = pairingDialog.getByLabel('已连接设备详情')
    await expect(deviceDetails).toContainText(deviceUrl.replace('http://', ''))
    await expect(deviceDetails).toContainText(deviceId)
    await pairingDialog.getByLabel('四位配对码').fill('4827')
    await pairingDialog.getByRole('button', { name: '配对设备' }).click()

    await expect(page.getByText('LAN 设备已连接')).toBeVisible()
    await expect
      .poll(() => requests.some((request) => request.path === '/api/v1/leases'))
      .toBe(true)

    await page.getByLabel('Dashboard target temperature').fill('235')
    await expect.poll(() => runtimeRequests()).toHaveLength(1)

    const pairingRequest = requests.find((request) => request.path === '/api/v1/pairing/claim')
    expect(pairingRequest).toMatchObject({ method: 'POST', body: { code: '4827' } })
    expect(requests.map((request) => request.path).join('\n')).not.toContain(token)
    expect(runtimeRequests()[0]).toMatchObject({
      headers: {
        authorization: `Bearer ${token}`,
        'x-flux-purr-lease': 'lan-lease-e2e',
      },
      body: { targetTempC: 235 },
    })

    expect(
      await page.evaluate(() =>
        Object.keys(window.localStorage).filter((key) => key.startsWith('flux-purr:lan-device:'))
      )
    ).toEqual([`flux-purr:lan-device:${deviceUrl}`])

    await page.reload()
    await expect
      .poll(
        () =>
          requests.filter(
            (request) => request.path === '/api/v1/identity' && request.headers.authorization
          ).length,
        { timeout: 10_000 }
      )
      .toBeGreaterThan(1)
    await page.getByRole('button', { name: '目标设备' }).click()
    const targetPicker = page.getByRole('dialog', { name: '设备与连接方式' })
    const lanConnection = targetPicker.getByRole('button', {
      name: 'WiFi / LAN · 192.168.1.18 · flux-purr-001122334455',
    })
    await expect(lanConnection).toBeVisible()
    await lanConnection.click()
    await expect(page.getByRole('button', { name: '目标设备' })).toContainText(
      'flux-purr-001122334455'
    )
    expect(requests.filter((request) => request.path === '/api/v1/pairing/claim')).toHaveLength(1)
  })

  test('keeps browser CIDR scanning visible and restores the last explicit range after reload', async ({
    page,
  }) => {
    await page.goto('/?demo=false')
    await openLanPairing(page)

    const cidr = page.getByLabel('CIDR 网段')
    const address = page.getByLabel('设备地址')
    await expect(cidr).toBeVisible()
    await address.fill(deviceUrl)
    await expect(page.getByRole('button', { name: '扫描设备' })).toHaveCount(0)
    await cidr.fill('192.168.31.0/24')

    await page.reload()
    await openLanPairing(page)
    await expect(page.getByLabel('设备地址')).toHaveValue(deviceUrl)
    await expect(page.getByLabel('CIDR 网段')).toHaveValue('192.168.31.0/24')
  })

  test('continues without a dialog when the connected device is code-exempt', async ({ page }) => {
    pairingMode = 'optional'
    await page.goto('/?demo=false')
    await openLanPairing(page)

    await page.getByLabel('设备地址').fill(deviceUrl)
    await page.getByRole('button', { name: '连接设备' }).click()

    await expect(page.getByText('LAN 设备已连接')).toBeVisible()
    await expect(page.getByLabel('四位配对码')).toHaveCount(0)
    await expect(page.getByRole('dialog', { name: '输入 LAN 配对码' })).toHaveCount(0)
    expect(requests.find((request) => request.path === '/api/v1/pairing/claim')).toMatchObject({
      method: 'POST',
      body: {},
    })
  })

  test('keeps an unavailable pairing device at public low-frequency information', async ({
    page,
  }) => {
    pairingMode = 'unavailable'
    await page.goto('/?demo=false')
    await openLanPairing(page)

    await page.getByLabel('设备地址').fill(deviceUrl)
    await page.getByRole('button', { name: '连接设备' }).click()

    await expect(
      page.getByText('已连接 flux-purr-001122334455。此设备仅提供基础低频信息读取。')
    ).toBeVisible()
    await expect(page.getByLabel('基础设备信息')).toBeVisible()
    await expect(page.getByLabel('四位配对码')).toHaveCount(0)
    expect(requests.filter((request) => request.path === '/api/v1/pairing/claim')).toHaveLength(0)
  })

  test('reports a browser-private-network failure without sending credentials in a URL', async ({
    page,
  }) => {
    await page.unroute(`${deviceUrl}/**`)
    await page.route(`${deviceUrl}/**`, (route) => route.abort('failed'))
    await page.goto('/?demo=false')
    await openLanPairing(page)

    await page.getByLabel('设备地址').fill(deviceUrl)
    await page.getByRole('button', { name: '连接设备' }).click()

    await expect(
      page.getByText(
        '浏览器阻止了对私网设备的访问。请确认使用 Chrome、Chromium 或 Edge，并允许私网访问。'
      )
    ).toBeVisible()
  })

  for (const [failureCode, message] of [
    ['pairing_code_invalid', '四位配对码不正确，请核对设备屏幕。'],
    ['pairing_locked', '该配对窗口已因连续失败锁定。请离开并重新进入 WiFi Info 页面获取新码。'],
  ] as const) {
    test(`reports ${failureCode} without probing an unpaired device`, async ({ page }) => {
      pairingFailureCode = failureCode
      await page.goto('/?demo=false')
      await openLanPairing(page)

      await pairRequiredLanDevice(page)

      await expect(page.getByText(message)).toBeVisible()
      expect(requests.filter((request) => request.path === '/api/v1/identity')).toHaveLength(0)
    })
  }

  test('does not offer direct LAN pairing or send requests in Safari', async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(Navigator.prototype, 'userAgent', {
        configurable: true,
        get: () => 'Mozilla/5.0 Version/18.0 Safari/605.1.15',
      })
    })
    await page.goto('/?demo=false')
    await openLanPairing(page)

    await expect(
      page.getByText(
        '此浏览器不支持 HTTPS 页面直连 HTTP 私网设备。请使用 Chrome、Chromium 或 Edge。'
      )
    ).toBeVisible()
    expect(requests).toHaveLength(0)
  })

  test('keeps runtime controls disabled when the LAN control lease is busy', async ({ page }) => {
    rejectLease = true
    await page.goto('/?demo=false')
    await openLanPairing(page)

    const targetSelector = page.getByRole('button', { name: '目标设备' })

    await pairRequiredLanDevice(page)

    await expect(page.getByText('LAN 租约获取失败')).toBeVisible()
    await expect(targetSelector).toContainText('DEVD')
    await expect(targetSelector).not.toContainText('flux-purr-001122334455')
    expect(runtimeRequests()).toHaveLength(0)
  })

  test('reads the current LAN state after a stale write and does not replay it', async ({
    page,
  }) => {
    await page.goto('/?demo=false')
    await openLanPairing(page)
    await pairRequiredLanDevice(page)
    await expect(page.getByText('LAN 设备已连接')).toBeVisible()

    rejectNextRuntimeAsStale = true
    await page.getByLabel('Dashboard target temperature').fill('155')

    await expect(page.getByText('LAN runtime update failed')).toBeVisible()
    await expect(
      page.getByText('设备控制状态已变化，已读取最新状态；请确认后重新提交。')
    ).toBeVisible()
    await expect(page.getByLabel('Dashboard target temperature')).toHaveValue('150')
    expect(runtimeRequests()).toHaveLength(1)
    await expect(page.getByText('Target updated')).toHaveCount(0)
  })

  test('keeps the remembered device but invalidates its rejected LAN credential', async ({
    page,
  }) => {
    await page.goto('/?demo=false')
    await openLanPairing(page)

    await page.evaluate(() => {
      window.localStorage.setItem(
        'flux-purr:lan-device:http://192.168.1.17',
        JSON.stringify({
          baseUrl: 'http://192.168.1.17',
          token: 'b'.repeat(64),
          deviceId: '001122334456',
          hostname: 'flux-purr-001122334456',
        })
      )
    })
    await pairRequiredLanDevice(page)
    await expect(page.getByText('LAN 设备已连接')).toBeVisible()

    await page.route('http://192.168.1.17/**', async (route) => {
      const requestOrigin = route.request().headers().origin ?? 'http://127.0.0.1:4173'
      await route.fulfill({
        status: 503,
        headers: jsonHeaders(requestOrigin),
        body: JSON.stringify({
          error: { code: 'device_unavailable', message: 'Device is unavailable.' },
        }),
      })
    })
    rejectBearerRequests = true
    await page.reload()

    await expect(page.getByText('LAN 配对凭据已失效')).toBeVisible()
    expect(
      await page.evaluate(() => {
        const raw = window.localStorage.getItem('flux-purr:lan-device:http://192.168.1.18')
        return {
          keys: Object.keys(window.localStorage)
            .filter((key) => key.startsWith('flux-purr:lan-device:'))
            .sort(),
          rejected: raw ? JSON.parse(raw) : null,
        }
      })
    ).toEqual({
      keys: [
        'flux-purr:lan-device:http://192.168.1.17',
        'flux-purr:lan-device:http://192.168.1.18',
      ],
      rejected: expect.objectContaining({
        baseUrl: 'http://192.168.1.18',
        deviceId,
        authorizationState: 'invalid',
      }),
    })
  })

  test('shows route recovery actions when a remembered LAN target is unavailable', async ({
    page,
  }) => {
    await page.goto('/?demo=false')
    await page.evaluate(
      ({ routedSession, backgroundSession }) => {
        window.localStorage.setItem(
          `flux-purr:lan-device:${routedSession.baseUrl}`,
          JSON.stringify(routedSession)
        )
        window.localStorage.setItem(
          `flux-purr:lan-device:${backgroundSession.baseUrl}`,
          JSON.stringify(backgroundSession)
        )
      },
      {
        routedSession: {
          baseUrl: deviceUrl,
          token,
          deviceId,
          hostname: 'flux-purr-001122334455',
        },
        backgroundSession: {
          baseUrl: 'http://192.168.1.19',
          token: 'b'.repeat(64),
          deviceId: '001122334457',
          hostname: 'flux-purr-001122334457',
          authorizationState: 'invalid',
        },
      }
    )
    rejectHealth = true

    await page.goto(`/devices/${deviceId}/overview?demo=false`)
    await expect(page.getByRole('heading', { name: '目标设备暂不可用' })).toBeVisible()
    await expect(page.getByRole('button', { name: '重试发现' })).toBeVisible()
    await expect(page.getByText('LAN 配对凭据已失效')).toHaveCount(0)
  })

  test('disables writes when a LAN lease heartbeat expires', async ({ page }) => {
    leaseTtlMs = 1_000
    await page.goto('/?demo=false')
    await openLanPairing(page)

    await pairRequiredLanDevice(page)
    await expect(page.getByText('LAN 设备已连接')).toBeVisible()

    rejectHeartbeat = true
    await expect(page.getByText('硬件连接受阻')).toBeVisible({ timeout: 5_000 })
    await expect(
      page.getByText('LAN lease 心跳失败：The LAN control lease expired.').first()
    ).toBeVisible()
    await expect(page.getByLabel('Dashboard target temperature')).toBeDisabled()
    expect(runtimeRequests()).toHaveLength(0)
  })

  function runtimeRequests() {
    return requests.filter((request) => request.path === '/api/v1/runtime')
  }
})

async function openLanPairing(page: Page) {
  await page.getByRole('button', { name: '目标设备' }).click()
  await page.getByRole('button', { name: '添加设备' }).click()
  await expect(page.getByRole('button', { name: /WiFi/ })).toHaveAttribute('aria-pressed', 'true')
  await expect(page.getByLabel('WiFi LAN pairing')).toBeVisible()
}

async function pairRequiredLanDevice(page: Page) {
  await page.getByLabel('设备地址').fill(deviceUrl)
  await page.getByRole('button', { name: '连接设备' }).click()
  const dialog = page.getByRole('dialog', { name: '输入 LAN 配对码' })
  await expect(dialog).toBeVisible()
  await dialog.getByLabel('四位配对码').fill('4827')
  await dialog.getByRole('button', { name: '配对设备' }).click()
}

function corsHeaders(allowedOrigin: string) {
  return {
    'access-control-allow-origin': allowedOrigin,
    'access-control-allow-methods': 'GET, POST, PUT, DELETE, OPTIONS',
    'access-control-allow-headers':
      'Authorization, Content-Type, X-Flux-Purr-Lease, X-Flux-Purr-Revision',
    'access-control-expose-headers': 'X-Flux-Purr-Revision',
    'access-control-allow-private-network': 'true',
  }
}

function jsonHeaders(allowedOrigin: string, revision = 7) {
  return {
    ...corsHeaders(allowedOrigin),
    'content-type': 'application/json',
    'X-Flux-Purr-Revision': String(revision),
  }
}

function identity() {
  return {
    deviceId,
    firmwareVersion: 'fw/e2e',
    buildId: 'e2e',
    gitSha: 'e2e',
    board: 'esp32s3',
    apiVersion: 'v1',
    protocolVersion: 'usb.v1',
    hostname: 'flux-purr-001122334455',
    capabilities: ['identity', 'network', 'status', 'runtime', 'calibration', 'heater_curve'],
  }
}

function network() {
  return {
    state: 'connected',
    ssid: 'FluxPurr-E2E',
    ip: '192.168.1.18',
    gateway: '192.168.1.1',
    dns: ['192.168.1.1'],
    wifiRssi: -48,
  }
}

function status(currentTargetC: number) {
  return {
    mode: 'idle',
    uptimeSeconds: 42,
    currentTempC: 25,
    targetTempC: currentTargetC,
    heaterEnabled: false,
    heaterOutputPercent: 0,
    activeCoolingEnabled: true,
    fanDisplayState: 'AUTO',
    fanEnabled: true,
    fanPwmPermille: 400,
    voltageMv: 20_000,
    currentMa: 0,
    boardTempCenti: 2500,
    pdRequestMv: 20_000,
    pdContractMv: 20_000,
    pdState: 'ready',
    calibration: {
      mode: 'off',
      ppsEnabled: false,
      heaterEnabled: false,
      stable: false,
      job: { status: 'idle', progressPercent: 0, samplesCollected: 0 },
    },
    network: network(),
  }
}
