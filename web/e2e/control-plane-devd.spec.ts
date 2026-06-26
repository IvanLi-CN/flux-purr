import http from 'node:http'
import { expect, test } from '@playwright/test'
import type {
  CalibrationChannel,
  CalibrationJobState,
  CalibrationPackage,
  CalibrationRuntimeState,
  CalibrationState,
  ControlPlaneStatus,
  HeaterCurvePackage,
  HeaterCurveState,
  NetworkSummary,
} from '../src/features/control-plane-demo/contracts'

const devdPort = Number(process.env.E2E_DEVD_PORT ?? 30081)
const devdBaseUrl = `http://127.0.0.1:${devdPort}`
const artifactPath = 'firmware/target/xtensa-esp32s3-none-elf/release/flux-purr'
const artifactSha = 'sha256:e2e'
const deviceId = 'serial-e2e'

test.describe('control plane live devd bridge', () => {
  let server: http.Server
  const requests: Array<{ method: string; path: string; body: unknown }> = []
  const sseClients = new Set<http.ServerResponse>()
  let listDevicesCallCount = 0
  let failDeviceList = false
  let missingAuthorizedPort = false
  let injectStatusTimeoutEvent = false
  let runtimeStatus = status(network('connected'))
  let calibrationState = calibration()
  let heaterCurveState = heaterCurve()

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
        if (failDeviceList) {
          sendJson(response, 503, {
            error: { code: 'devd_unavailable', message: 'Failed to fetch', retryable: true },
          })
          return
        }
        listDevicesCallCount += 1
        const nativeConnection = listDevicesCallCount === 1 ? 'busy' : 'disconnected'
        if (missingAuthorizedPort) {
          sendJson(response, 200, {
            devices: [
              {
                id: deviceId,
                displayName: 'Authorized serial device',
                portPath: '/dev/cu.usbmodem21231401',
                transport: 'native_serial',
                connection: 'error',
                identity: identity([
                  'identity',
                  'status',
                  'network',
                  'wifi_config',
                  'monitor',
                  'flash',
                ]),
                network: {
                  state: 'error',
                  ssid: null,
                  ip: null,
                  gateway: null,
                  dns: [],
                  wifiRssi: null,
                  lastError:
                    'Authorized serial port /dev/cu.usbmodem21231401 is missing. Observed alternate Espressif serial ports: /dev/cu.usbmodem212101, /dev/cu.usbmodem212201.',
                },
                status: status({
                  state: 'error',
                  ssid: null,
                  ip: null,
                  gateway: null,
                  dns: [],
                  wifiRssi: null,
                  lastError:
                    'Authorized serial port /dev/cu.usbmodem21231401 is missing. Observed alternate Espressif serial ports: /dev/cu.usbmodem212101, /dev/cu.usbmodem212201.',
                }),
                events: [
                  {
                    id: 'event-e2e-port-missing',
                    timestamp: '1002',
                    deviceId,
                    kind: 'serial',
                    message: 'authorized serial port missing',
                    payload: {
                      code: 'authorized_port_missing',
                      portPath: '/dev/cu.usbmodem21231401',
                      candidates: ['/dev/cu.usbmodem212101', '/dev/cu.usbmodem212201'],
                    },
                  },
                ],
              },
            ],
          })
          return
        }
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
              connection: nativeConnection,
              identity: identity([
                'identity',
                'status',
                'network',
                'wifi_config',
                'monitor',
                'flash',
              ]),
              network: network('idle'),
              status: withStatusNetwork(runtimeStatus, network('idle')),
              calibration: cloneCalibrationState(calibrationState),
              heaterCurve: cloneHeaterCurveState(heaterCurveState),
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
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        sendJson(
          response,
          200,
          identity(['identity', 'status', 'network', 'usb_jsonl', 'wifi_config', 'monitor'])
        )
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/network`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        sendJson(response, 200, network('connected'))
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/status`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        sendJson(response, 200, runtimeStatus)
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/calibration`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        sendJson(response, 200, cloneCalibrationState(calibrationState))
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/calibration/job`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        sendJson(response, 200, cloneCalibrationJob(runtimeStatus.calibration.job))
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/heater-curve`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        sendJson(response, 200, cloneHeaterCurveState(heaterCurveState))
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/events`) {
        if (missingAuthorizedPort) {
          sendSse(response, sseClients, {
            id: 'event-e2e-port-missing-stream',
            timestamp: '1003',
            deviceId,
            kind: 'serial',
            message: 'authorized serial port missing',
            payload: {
              code: 'authorized_port_missing',
              portPath: '/dev/cu.usbmodem21231401',
              candidates: ['/dev/cu.usbmodem212101', '/dev/cu.usbmodem212201'],
            },
          })
          return
        }
        if (injectStatusTimeoutEvent) {
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
        sendSse(response, sseClients)
        return
      }

      if (method === 'PUT' && url.pathname === `/api/v1/devices/${deviceId}/wifi`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
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
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        runtimeStatus = applyRuntimeRequest(runtimeStatus, body)
        sendJson(response, 200, runtimeStatus)
        return
      }

      if (method === 'PUT' && url.pathname === `/api/v1/devices/${deviceId}/calibration`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        calibrationState = applyCalibrationRequest(calibrationState, runtimeStatus, body)
        sendJson(response, 200, cloneCalibrationState(calibrationState))
        return
      }

      if (method === 'POST' && url.pathname === `/api/v1/devices/${deviceId}/calibration/job`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        runtimeStatus = applyCalibrationJobRequest(runtimeStatus, body)
        sendJson(response, 200, cloneCalibrationJob(runtimeStatus.calibration.job))
        return
      }

      if (method === 'POST' && url.pathname === `/api/v1/devices/${deviceId}/calibration/apply`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        calibrationState = applyCalibration(calibrationState)
        sendJson(response, 200, cloneCalibrationState(calibrationState))
        return
      }

      if (method === 'PUT' && url.pathname === `/api/v1/devices/${deviceId}/heater-curve`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        heaterCurveState = applyHeaterCurveRequest(heaterCurveState, body)
        sendJson(response, 200, cloneHeaterCurveState(heaterCurveState))
        return
      }

      if (method === 'POST' && url.pathname === `/api/v1/devices/${deviceId}/heater-curve/save`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        heaterCurveState = saveHeaterCurve(heaterCurveState)
        sendJson(response, 200, cloneHeaterCurveState(heaterCurveState))
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
    listDevicesCallCount = 0
    failDeviceList = false
    missingAuthorizedPort = false
    injectStatusTimeoutEvent = false
    runtimeStatus = status(network('connected'))
    calibrationState = calibration()
    heaterCurveState = heaterCurve()
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
    await expect(page.getByLabel('Transport capabilities').getByText('connected')).toBeVisible()

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

  test('keeps the live workspace visible while devd is still reclaiming the first native probe', async ({
    page,
  }) => {
    await page.goto('/?demo=false')

    await expect(page.getByRole('combobox', { name: '目标设备' })).toContainText('/ DEVD')
    await expect(
      page.getByLabel('Transport capabilities').getByText('正在重新接管本机 devd 租约，请稍候。')
    ).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Choose target' })).toHaveCount(0)
    await expect(page.getByText('No known devices')).toHaveCount(0)

    await page.waitForTimeout(2500)
    await expect(
      page.getByLabel('Transport capabilities').getByText('正在重新接管本机 devd 租约，请稍候。')
    ).toHaveCount(0)
    await expect(page.getByText('运行时已同步')).toBeVisible()
    await expect(page.getByText('有效')).toBeVisible()
  })

  test('preserves the chosen calibration tab and blocks calibration controls while devd is still reacquiring the lease', async ({
    page,
  }) => {
    await page.goto('/?demo=false')

    await page.getByRole('button', { name: /校准/i }).click()
    await page.getByRole('tab', { name: '温度标定' }).click()

    const calibrationModeToggle = page.getByRole('switch', { name: '标定模式' })
    await expect(calibrationModeToggle).toBeDisabled()
    await expect(page.getByRole('heading', { name: '温度 ADC' })).toBeVisible()

    await expect(page.getByText('运行时已同步')).toBeVisible({ timeout: 10_000 })
    await expect(page.getByText('有效')).toBeVisible({ timeout: 10_000 })
    await expect(page.getByRole('heading', { name: '温度 ADC' })).toBeVisible()
    await expect(page.getByRole('heading', { name: '加热曲线' })).toHaveCount(0)
    await expect(calibrationModeToggle).toBeEnabled()
  })

  test('updates the RTD calibration target after heater start instead of leaving the old target latched', async ({
    page,
  }) => {
    await page.goto('/?demo=false')

    await page.getByRole('button', { name: /校准/i }).click()
    await page.getByRole('tab', { name: '温度标定' }).click()
    await expect(page.getByText('运行时已同步')).toBeVisible({ timeout: 10_000 })

    const calibrationModeToggle = page.getByRole('switch', { name: '标定模式' })
    await expect(calibrationModeToggle).toBeEnabled()

    const targetAdcInput = page.getByLabel('目标 ADC 输入')
    await targetAdcInput.fill('950')
    await calibrationModeToggle.click()
    await page.waitForTimeout(700)

    await page.getByRole('button', { name: '申请 PPS' }).click()
    await page.waitForTimeout(700)

    await page.getByRole('button', { name: '开启加热' }).click()
    await page.waitForTimeout(700)

    await targetAdcInput.fill('980')
    await page.waitForTimeout(1_200)

    await expect
      .poll(
        () =>
          runtimeRequests().filter(
            (request) =>
              (request.body as { calibration?: { targetAdcMv?: number } } | null)?.calibration
                ?.targetAdcMv === 980
          ).length
      )
      .toBeGreaterThanOrEqual(1)
    await expect(page.getByRole('heading', { name: '温度 ADC' })).toBeVisible()
    await expect(targetAdcInput).toHaveValue('980')
  })

  test('keeps dashboard target temperature writable after RTD calibration heater start', async ({
    page,
  }) => {
    await page.goto('/?demo=false')

    await page.getByRole('button', { name: /校准/i }).click()
    await page.getByRole('tab', { name: '温度标定' }).click()
    await expect(page.getByText('运行时已同步')).toBeVisible({ timeout: 10_000 })

    const calibrationModeToggle = page.getByRole('switch', { name: '标定模式' })
    const targetAdcInput = page.getByLabel('目标 ADC 输入')

    await targetAdcInput.fill('950')
    await calibrationModeToggle.click()
    await page.waitForTimeout(700)
    await page.getByRole('button', { name: '申请 PPS' }).click()
    await page.waitForTimeout(700)
    await page.getByRole('button', { name: '开启加热' }).click()
    await page.waitForTimeout(700)

    await page.getByRole('button', { name: /总览/i }).click()
    await expect(page.getByText('请先关闭校准控制')).toBeVisible()
    await page.getByRole('button', { name: '关闭并继续' }).click()
    await page.waitForTimeout(500)
    await page.getByRole('button', { name: /总览/i }).click()
    const dashboardTarget = page.getByLabel('Dashboard target temperature')

    await dashboardTarget.fill('50')
    await page.waitForTimeout(1_000)
    await expect
      .poll(
        () =>
          runtimeRequests().filter(
            (request) =>
              typeof (request.body as { targetTempC?: number } | null)?.targetTempC === 'number' &&
              (request.body as { targetTempC?: number }).targetTempC === 50
          ).length
      )
      .toBeGreaterThanOrEqual(1)
    await expect(dashboardTarget).toHaveValue('50')

    await dashboardTarget.fill('55')
    await page.waitForTimeout(1_000)
    await expect
      .poll(
        () =>
          runtimeRequests().filter(
            (request) =>
              typeof (request.body as { targetTempC?: number } | null)?.targetTempC === 'number' &&
              (request.body as { targetTempC?: number }).targetTempC === 55
          ).length
      )
      .toBeGreaterThanOrEqual(1)
    await expect(dashboardTarget).toHaveValue('55')
  })

  test('sends runtime commands through the active devd lease', async ({ page }) => {
    await page.goto('/?demo=false')

    await expect(page.getByRole('combobox', { name: '目标设备' })).toContainText('/ DEVD')
    await expect(page.getByText('运行时已同步')).toBeVisible()

    await page.getByRole('button', { name: /总览/i }).click()
    await page.getByLabel('Dashboard target temperature').fill('235')
    await expect(page.getByText('Target updated')).toBeVisible()

    await page.getByRole('button', { name: /设置/i }).click()
    await page.getByRole('button', { name: 'OFF' }).click()
    await expect(page.getByText('Fan policy updated', { exact: true })).toBeVisible()

    await page.getByRole('button', { name: /总览/i }).click()
    await page.getByRole('button', { name: 'Hold heater' }).click()
    await expect(page.getByText('Heater hold requested')).toBeVisible()

    expect(wifiRequests()).toHaveLength(0)
    await expect.poll(() => runtimeRequests().length).toBeGreaterThanOrEqual(3)
    await expect
      .poll(() =>
        page
          .locator('.industrial-action-feedback strong')
          .textContent()
          .then((value) => value?.trim() ?? '')
      )
      .toBe('Heater held')
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

  test('keeps the live devd workspace visible across repeated reloads', async ({ page }) => {
    await page.goto('/?demo=false')

    for (let reloadIndex = 0; reloadIndex < 3; reloadIndex += 1) {
      await expect(page.getByRole('combobox', { name: '目标设备' })).toContainText('/ DEVD')
      await expect(page.getByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
      await expect(page.getByRole('heading', { name: 'Choose target' })).toHaveCount(0)
      await expect(page.getByText('No known devices')).toHaveCount(0)
      await expect(page.getByText('Failed to fetch')).toHaveCount(0)

      await page.reload()
      await expect(page.getByRole('combobox', { name: '目标设备' })).toContainText('/ DEVD')
      await expect(
        page.getByLabel('Transport capabilities').getByText('正在重新接管本机 devd 租约，请稍候。')
      ).toBeVisible()
      await expect(page.getByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
      await expect(page.getByRole('heading', { name: 'Choose target' })).toHaveCount(0)
      await expect(page.getByText('No known devices')).toHaveCount(0)
      await expect(page.getByText('Failed to fetch')).toHaveCount(0)
      await page.waitForTimeout(2500)
      await expect(page.getByText('运行时已同步')).toBeVisible()
      await expect(page.getByText('有效')).toBeVisible()
      await expect(page.getByText('Lease conflict')).toHaveCount(0)
      await expect(page.getByText('lease_conflict')).toHaveCount(0)
    }
  })

  test('keeps a devd bridge placeholder when the device list refresh fails', async ({ page }) => {
    injectStatusTimeoutEvent = true
    failDeviceList = true

    await page.goto('/?demo=false')

    const targetRegion = page.getByRole('region', { name: '当前目标' })
    await expect(page.getByRole('combobox', { name: '目标设备' })).toContainText('/ DEVD')
    await expect(targetRegion).toContainText('传输')
    await expect(targetRegion).toContainText('DEVD')
    await expect(targetRegion).toContainText('租约')
    await expect(targetRegion).toContainText('无')
    await expect(page.getByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Choose target' })).toHaveCount(0)
    await expect(page.getByText('No known devices')).toHaveCount(0)
    await expect(
      page.getByLabel('Transport capabilities').getByText('Failed to fetch')
    ).toBeVisible()
    injectStatusTimeoutEvent = false
  })

  test('surfaces the missing authorized serial port instead of falling back to the empty chooser', async ({
    page,
  }) => {
    missingAuthorizedPort = true

    await page.goto('/?demo=false')

    const targetRegion = page.getByRole('region', { name: '当前目标' })
    await expect(page.getByRole('combobox', { name: '目标设备' })).toContainText('/ DEVD')
    await expect(targetRegion).toContainText('传输')
    await expect(targetRegion).toContainText('DEVD')
    await expect(targetRegion).toContainText('租约')
    await expect(targetRegion).toContainText('有效')
    await expect(page.getByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Choose target' })).toHaveCount(0)
    await expect(page.getByText('No known devices')).toHaveCount(0)
    await expect(page.getByText('Failed to fetch')).toHaveCount(0)
    await expect(
      page
        .getByLabel('Transport capabilities')
        .getByText('Authorized serial port /dev/cu.usbmodem21231401 is missing.')
    ).toBeVisible()
    await expect(
      page
        .getByLabel('Transport capabilities')
        .getByText(
          'Authorized serial port /dev/cu.usbmodem21231401 is missing. Observed alternate Espressif serial ports: /dev/cu.usbmodem212101, /dev/cu.usbmodem212201.'
        )
    ).toBeVisible()
    await expect(
      page.getByText(
        'Authorized serial port /dev/cu.usbmodem21231401 is missing. Observed alternate Espressif serial ports: /dev/cu.usbmodem212101, /dev/cu.usbmodem212201.'
      )
    ).toHaveCount(2)
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
  event?: Record<string, unknown>
) {
  response.writeHead(200, {
    'access-control-allow-origin': '*',
    'cache-control': 'no-cache',
    connection: 'keep-alive',
    'content-type': 'text/event-stream',
  })
  if (event) {
    response.write(`event: ${event.kind}\n`)
    response.write(`data: ${JSON.stringify(event)}\n\n`)
  }
  clients.add(response)
  response.on('close', () => clients.delete(response))
}

function sendMissingAuthorizedPortError(response: http.ServerResponse) {
  sendJson(response, 503, {
    error: {
      code: 'serial_open_failed',
      message: 'Failed to open serial port: No such file or directory',
      retryable: true,
    },
  })
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

function status(networkSummary: NetworkSummary): ControlPlaneStatus {
  return {
    mode: 'sampling',
    uptimeSeconds: 42,
    currentTempC: 181.5,
    targetTempC: 220,
    selectedPresetSlot: 5,
    presetsC: [50, 100, 120, 150, 180, 220, 210, 230, 250, 300],
    heaterEnabled: true,
    heaterOutputPercent: 18,
    activeCoolingEnabled: true,
    fanDisplayState: 'AUTO',
    fanEnabled: true,
    fanPwmPermille: 500,
    rtdRawAdcMv: 1123,
    vinRawAdcMv: 1678,
    voltageMv: 20000,
    currentMa: 720,
    boardTempCenti: 3600,
    pdRequestMv: 20000,
    pdContractMv: 20000,
    pdState: 'ready',
    manualPpsEnabled: false,
    manualPpsMv: null,
    manualPpsMa: null,
    ppsCapabilityMinMv: 5000,
    ppsCapabilityMaxMv: 20000,
    ppsCapabilityMaxMa: 3000,
    manualPpsError: null,
    heaterLockReason: null,
    calibration: calibrationRuntimeState(),
    frontpanelKey: null,
    network: networkSummary,
  }
}

function calibrationRuntimeState(): CalibrationRuntimeState {
  return {
    mode: 'off',
    ppsEnabled: false,
    ppsMv: null,
    ppsMa: null,
    heaterEnabled: false,
    targetAdcMv: null,
    stable: false,
    stabilityErrorMv: null,
    error: null,
    job: {
      kind: null,
      status: 'idle',
      progressPercent: 0,
      samplesCollected: 0,
      nextRequestMv: null,
      message: null,
    },
  }
}

function calibration(): CalibrationState {
  const active: CalibrationPackage = {
    rtdAdc: [
      { observedMv: 1123, expectedMv: 980, referenceTempC: 25 },
      { observedMv: 1188, expectedMv: 1120, referenceTempC: 60 },
      null,
      null,
      null,
      null,
      null,
      null,
    ],
    vinAdc: [
      { observedMv: 1678, expectedMv: 20000, referenceVinMv: 20000 },
      null,
      null,
      null,
      null,
      null,
      null,
      null,
    ],
  }
  return {
    active: cloneCalibrationPackage(active),
    draft: cloneCalibrationPackage(active),
    activeFit: createCalibrationFits(active),
    draftFit: createCalibrationFits(active),
  }
}

function heaterCurve(): HeaterCurveState {
  return {
    active: {
      points: [
        { tempCentiC: 2120, resistanceMilliohms: 4251 },
        { tempCentiC: 5180, resistanceMilliohms: 4732 },
        null,
        null,
        null,
        null,
        null,
        null,
      ],
    },
    preview: null,
  }
}

function withStatusNetwork(
  currentStatus: ControlPlaneStatus,
  networkSummary: NetworkSummary
): ControlPlaneStatus {
  return {
    ...currentStatus,
    calibration: cloneCalibrationRuntimeState(currentStatus.calibration),
    network: { ...networkSummary },
  }
}

function applyRuntimeRequest(currentStatus: ControlPlaneStatus, body: unknown): ControlPlaneStatus {
  const calibrationPatch = recordValue(bodyField(body, 'calibration'))
  const nextCalibration = calibrationPatch
    ? applyCalibrationRuntimeRequest(currentStatus.calibration, calibrationPatch)
    : cloneCalibrationRuntimeState(currentStatus.calibration)
  const topLevelHeaterEnabled =
    typeof bodyField(body, 'heaterEnabled') === 'boolean'
      ? (bodyField(body, 'heaterEnabled') as boolean)
      : currentStatus.heaterEnabled
  const calibrationHeaterEnabled =
    calibrationPatch && typeof calibrationPatch.heaterEnabled === 'boolean'
      ? calibrationPatch.heaterEnabled
      : undefined
  const heaterEnabled = calibrationHeaterEnabled ?? topLevelHeaterEnabled
  const manualPpsEnabled =
    typeof bodyField(body, 'manualPpsEnabled') === 'boolean'
      ? (bodyField(body, 'manualPpsEnabled') as boolean)
      : (currentStatus.manualPpsEnabled ?? false)
  const manualPpsMv =
    typeof bodyField(body, 'manualPpsMv') === 'number'
      ? (bodyField(body, 'manualPpsMv') as number)
      : manualPpsEnabled
        ? (currentStatus.manualPpsMv ?? 9000)
        : null
  const manualPpsMa =
    typeof bodyField(body, 'manualPpsMa') === 'number'
      ? (bodyField(body, 'manualPpsMa') as number)
      : manualPpsEnabled
        ? (currentStatus.manualPpsMa ?? 2000)
        : null

  return {
    ...currentStatus,
    targetTempC:
      typeof bodyField(body, 'targetTempC') === 'number'
        ? (bodyField(body, 'targetTempC') as number)
        : currentStatus.targetTempC,
    selectedPresetSlot:
      typeof bodyField(body, 'selectedPresetSlot') === 'number'
        ? (bodyField(body, 'selectedPresetSlot') as number)
        : currentStatus.selectedPresetSlot,
    presetsC: Array.isArray(bodyField(body, 'presetsC'))
      ? ((bodyField(body, 'presetsC') as Array<number | null>).map((value) =>
          typeof value === 'number' || value === null ? value : null
        ) as Array<number | null>)
      : currentStatus.presetsC,
    activeCoolingEnabled:
      typeof bodyField(body, 'activeCoolingEnabled') === 'boolean'
        ? (bodyField(body, 'activeCoolingEnabled') as boolean)
        : currentStatus.activeCoolingEnabled,
    fanDisplayState: bodyField(body, 'activeCoolingEnabled') === false ? 'OFF' : 'AUTO',
    heaterEnabled,
    heaterOutputPercent: heaterEnabled ? 18 : 0,
    manualPpsEnabled,
    manualPpsMv,
    manualPpsMa,
    calibration: nextCalibration,
    network: network('connected'),
  }
}

function applyCalibrationRuntimeRequest(
  current: CalibrationRuntimeState,
  patch: Record<string, unknown>
): CalibrationRuntimeState {
  const nextMode =
    typeof patch.mode === 'string' ? (patch.mode as CalibrationRuntimeState['mode']) : current.mode
  const nextPpsEnabled =
    typeof patch.ppsEnabled === 'boolean' ? patch.ppsEnabled : current.ppsEnabled
  const nextHeaterEnabled =
    nextMode === 'off'
      ? false
      : typeof patch.heaterEnabled === 'boolean'
        ? patch.heaterEnabled
        : current.heaterEnabled
  const nextTargetAdcMv =
    typeof patch.targetAdcMv === 'number' ? patch.targetAdcMv : (current.targetAdcMv ?? null)
  return {
    ...current,
    mode: nextMode,
    ppsEnabled: nextPpsEnabled,
    ppsMv: patch.ppsEnabled === false ? null : numberOrFallback(patch.ppsMv, current.ppsMv),
    ppsMa: patch.ppsEnabled === false ? null : current.ppsMa,
    heaterEnabled: nextHeaterEnabled,
    targetAdcMv: nextTargetAdcMv,
    stable:
      nextMode === 'rtd_adc' && nextPpsEnabled && nextHeaterEnabled && nextTargetAdcMv != null,
    stabilityErrorMv:
      nextMode === 'rtd_adc' && nextPpsEnabled && nextHeaterEnabled && nextTargetAdcMv != null
        ? 0
        : null,
    error: null,
    job:
      nextMode === 'off'
        ? {
            kind: null,
            status: 'idle',
            progressPercent: 0,
            samplesCollected: 0,
            nextRequestMv: null,
            message: null,
          }
        : cloneCalibrationJob(current.job),
  }
}

function applyCalibrationRequest(
  current: CalibrationState,
  currentStatus: ControlPlaneStatus,
  body: unknown
): CalibrationState {
  const op = bodyField(body, 'op')
  if (op === 'import') {
    const packageValue = calibrationPackageValue(bodyField(body, 'package'))
    if (!packageValue) {
      return cloneCalibrationState(current)
    }
    const normalized = cloneCalibrationPackage(packageValue)
    return {
      active: cloneCalibrationPackage(current.active),
      draft: normalized,
      activeFit: cloneCalibrationFits(current.activeFit),
      draftFit: createCalibrationFits(normalized),
    }
  }

  const channel = calibrationChannelValue(bodyField(body, 'channel'))
  if (!channel) {
    return cloneCalibrationState(current)
  }

  const nextDraft = cloneCalibrationPackage(current.draft)
  const samples = channel === 'rtd_adc' ? nextDraft.rtdAdc : nextDraft.vinAdc

  if (op === 'clear') {
    for (let index = 0; index < samples.length; index += 1) {
      samples[index] = null
    }
  } else if (op === 'delete') {
    const sampleIndex = numberOrFallback(bodyField(body, 'sampleIndex'), null)
    if (sampleIndex != null && sampleIndex >= 0 && sampleIndex < samples.length) {
      samples[sampleIndex] = null
    }
  } else if (op === 'capture') {
    const nextSample =
      channel === 'rtd_adc'
        ? {
            observedMv: currentStatus.rtdRawAdcMv ?? 0,
            expectedMv: Math.round(
              (numberOrFallback(bodyField(body, 'referenceTempC'), 0) ?? 0) * 14 + 630
            ),
            referenceTempC: numberOrFallback(bodyField(body, 'referenceTempC'), null) ?? undefined,
          }
        : {
            observedMv: currentStatus.vinRawAdcMv ?? 0,
            expectedMv:
              numberOrFallback(bodyField(body, 'referenceVinMv'), currentStatus.voltageMv) ?? 0,
            referenceVinMv:
              numberOrFallback(bodyField(body, 'referenceVinMv'), currentStatus.voltageMv) ?? 0,
          }
    const emptyIndex = samples.findIndex((sample) => sample == null)
    samples[emptyIndex === -1 ? samples.length - 1 : emptyIndex] = nextSample
  }

  return {
    active: cloneCalibrationPackage(current.active),
    draft: nextDraft,
    activeFit: cloneCalibrationFits(current.activeFit),
    draftFit: createCalibrationFits(nextDraft),
  }
}

function applyCalibration(current: CalibrationState): CalibrationState {
  const nextActive = cloneCalibrationPackage(current.draft)
  const nextFit = createCalibrationFits(nextActive)
  return {
    active: nextActive,
    draft: cloneCalibrationPackage(current.draft),
    activeFit: nextFit,
    draftFit: cloneCalibrationFits(current.draftFit),
  }
}

function applyCalibrationJobRequest(
  currentStatus: ControlPlaneStatus,
  body: unknown
): ControlPlaneStatus {
  const op = bodyField(body, 'op')
  const currentJob = currentStatus.calibration.job
  const nextJob: CalibrationJobState =
    op === 'start'
      ? {
          kind:
            bodyField(body, 'kind') === 'heater_curve_auto' ||
            bodyField(body, 'kind') === 'vin_adc_auto'
              ? (bodyField(body, 'kind') as CalibrationJobState['kind'])
              : null,
          status: 'running',
          progressPercent: 0,
          samplesCollected: 0,
          nextRequestMv: bodyField(body, 'kind') === 'vin_adc_auto' ? 12000 : 20000,
          message: null,
        }
      : {
          ...cloneCalibrationJob(currentJob),
          status: 'canceled',
          progressPercent: 0,
          nextRequestMv: null,
          message: 'Canceled by operator.',
        }

  return {
    ...currentStatus,
    calibration: {
      ...cloneCalibrationRuntimeState(currentStatus.calibration),
      job: nextJob,
    },
  }
}

function applyHeaterCurveRequest(current: HeaterCurveState, body: unknown): HeaterCurveState {
  const op = bodyField(body, 'op')
  if (op !== 'preview') {
    return {
      active: cloneHeaterCurvePackage(current.active),
      preview: null,
    }
  }

  const packageValue = heaterCurvePackageValue(bodyField(body, 'package'))
  if (!packageValue) {
    return cloneHeaterCurveState(current)
  }

  return {
    active: cloneHeaterCurvePackage(current.active),
    preview: normalizeHeaterCurvePackage(packageValue),
  }
}

function saveHeaterCurve(current: HeaterCurveState): HeaterCurveState {
  if (!current.preview) {
    return cloneHeaterCurveState(current)
  }
  return {
    active: cloneHeaterCurvePackage(current.preview),
    preview: null,
  }
}

function createCalibrationFits(calibrationPackage: CalibrationPackage) {
  return {
    rtdAdc: createCalibrationFit(calibrationPackage.rtdAdc, 'rtd_adc'),
    vinAdc: createCalibrationFit(calibrationPackage.vinAdc, 'vin_adc'),
  }
}

function createCalibrationFit(
  samples: Array<{ observedMv: number; expectedMv: number } | null>,
  channel: CalibrationChannel
) {
  const customSampleCount = samples.filter((sample) => sample != null).length
  return {
    gain: 1,
    offsetMv: 0,
    customSampleCount,
    defaultSampleCount: channel === 'rtd_adc' ? 2 : 2,
  }
}

function cloneCalibrationState(current: CalibrationState): CalibrationState {
  return {
    active: cloneCalibrationPackage(current.active),
    draft: cloneCalibrationPackage(current.draft),
    activeFit: cloneCalibrationFits(current.activeFit),
    draftFit: cloneCalibrationFits(current.draftFit),
  }
}

function cloneCalibrationPackage(current: CalibrationPackage): CalibrationPackage {
  return {
    rtdAdc: current.rtdAdc.map((sample) => (sample ? { ...sample } : null)),
    vinAdc: current.vinAdc.map((sample) => (sample ? { ...sample } : null)),
  }
}

function cloneCalibrationFits(current: CalibrationState['activeFit']) {
  return {
    rtdAdc: { ...current.rtdAdc },
    vinAdc: { ...current.vinAdc },
  }
}

function cloneCalibrationRuntimeState(current: CalibrationRuntimeState): CalibrationRuntimeState {
  return {
    ...current,
    job: cloneCalibrationJob(current.job),
  }
}

function cloneCalibrationJob(current: CalibrationJobState): CalibrationJobState {
  return { ...current }
}

function cloneHeaterCurveState(current: HeaterCurveState): HeaterCurveState {
  return {
    active: cloneHeaterCurvePackage(current.active),
    preview: current.preview ? cloneHeaterCurvePackage(current.preview) : null,
  }
}

function cloneHeaterCurvePackage(current: HeaterCurvePackage): HeaterCurvePackage {
  return {
    points: current.points.map((point) => (point ? { ...point } : null)),
  }
}

function normalizeHeaterCurvePackage(current: HeaterCurvePackage): HeaterCurvePackage {
  const points = current.points
    .filter((point): point is NonNullable<typeof point> => point != null)
    .map((point) => ({ ...point }))
    .sort((left, right) => left.tempCentiC - right.tempCentiC)

  return {
    points: Array.from({ length: 8 }, (_, index) => points[index] ?? null),
  }
}

function calibrationPackageValue(value: unknown): CalibrationPackage | null {
  const record = recordValue(value)
  if (!record || !Array.isArray(record.rtdAdc) || !Array.isArray(record.vinAdc)) {
    return null
  }
  return {
    rtdAdc: record.rtdAdc.map(normalizeCalibrationSample),
    vinAdc: record.vinAdc.map(normalizeCalibrationSample),
  }
}

function normalizeCalibrationSample(value: unknown) {
  const record = recordValue(value)
  if (!record) {
    return null
  }
  const observedMv = numberOrFallback(record.observedMv, null)
  const expectedMv = numberOrFallback(record.expectedMv, null)
  return observedMv == null || expectedMv == null ? null : { observedMv, expectedMv }
}

function heaterCurvePackageValue(value: unknown): HeaterCurvePackage | null {
  const record = recordValue(value)
  if (!record || !Array.isArray(record.points)) {
    return null
  }
  return {
    points: record.points.map((point) => {
      const currentPoint = recordValue(point)
      if (!currentPoint) {
        return null
      }
      const tempCentiC = numberOrFallback(currentPoint.tempCentiC, null)
      const resistanceMilliohms = numberOrFallback(currentPoint.resistanceMilliohms, null)
      return tempCentiC == null || resistanceMilliohms == null
        ? null
        : { tempCentiC, resistanceMilliohms }
    }),
  }
}

function calibrationChannelValue(value: unknown): CalibrationChannel | null {
  return value === 'rtd_adc' || value === 'vin_adc' ? value : null
}

function recordValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' ? (value as Record<string, unknown>) : null
}

function numberOrFallback(value: unknown, fallback: number | null) {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}
