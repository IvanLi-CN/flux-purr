import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import http from 'node:http'
import { resolve } from 'node:path'

import { expect, type Page, test } from '@playwright/test'
import { zipSync } from 'fflate'

import type {
  CalibrationChannel,
  CalibrationFit,
  CalibrationJobState,
  CalibrationRuntimeState,
  CalibrationSlotFit,
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
const e2eFirmwareAssetPath = 'firmware/releases/e2e/flux-purr-e2e.fluxpurr-fw'

interface E2eFirmwareCatalog {
  schemaVersion: 1
  generatedAt: string
  releaseCount: number
  releases: Array<{
    id: string
    version: string
    channel: 'local'
    source: 'local'
    releaseTag: null
    sourceSha: string
    buildId: string
    bundleSha256: string
    size: number
    assetPath: string
    target: 'ESP32-S3FH4R2'
    publishedAt: string
  }>
}

async function createE2eFirmwareBundle() {
  const bootloader = new Uint8Array(0x4000).fill(0x11)
  const sourcePartition = new Uint8Array(
    await readFile(resolve(import.meta.dirname, '../../firmware/partitions.bin'))
  )
  const partitionTable = new Uint8Array(0x1000).fill(0xff)
  partitionTable.set(sourcePartition)
  const app = new Uint8Array(0x5000).fill(0x33)
  const images = [bootloader, partitionTable, app]
  const manifestFixture = JSON.parse(
    await readFile(
      resolve(
        import.meta.dirname,
        '../../docs/specs/web-firmware-install-recovery/contracts/fixtures/valid-manifest.json'
      ),
      'utf8'
    )
  ) as {
    identity: {
      version: string
      sourceSha: string
      buildId: string
      channel: 'local'
    }
    segments: Array<{ length: number; sha256: string; md5: string }>
  }
  const manifest = structuredClone(manifestFixture) as typeof manifestFixture & {
    segments: Array<{ length: number; sha256: string; md5: string }>
  }
  manifest.identity = {
    version: '0.16.4',
    sourceSha: 'e'.repeat(40),
    buildId: 'e'.repeat(16),
    channel: 'local',
  }
  for (const [index, image] of images.entries()) {
    manifest.segments[index].length = image.byteLength
    manifest.segments[index].sha256 = `sha256:${createHash('sha256').update(image).digest('hex')}`
    manifest.segments[index].md5 = createHash('md5').update(image).digest('hex')
  }
  const bytes = zipSync({
    'manifest.json': new TextEncoder().encode(`${JSON.stringify(manifest)}\n`),
    'images/bootloader.bin': bootloader,
    'images/partition-table.bin': partitionTable,
    'images/factory-app.bin': app,
  })
  const bundleSha256 = `sha256:${createHash('sha256').update(bytes).digest('hex')}`
  const catalog: E2eFirmwareCatalog = {
    schemaVersion: 1,
    generatedAt: '2026-08-23T00:00:00Z',
    releaseCount: 1,
    releases: [
      {
        id: 'local:e2e',
        version: manifest.identity.version,
        channel: 'local',
        source: 'local',
        releaseTag: null,
        sourceSha: manifest.identity.sourceSha,
        buildId: manifest.identity.buildId,
        bundleSha256,
        size: bytes.byteLength,
        assetPath: e2eFirmwareAssetPath,
        target: 'ESP32-S3FH4R2',
        publishedAt: '2026-08-23T00:00:00Z',
      },
    ],
  }
  return { bytes, catalog }
}

test.describe('control plane live devd bridge', () => {
  let server: http.Server
  const requests: Array<{ method: string; path: string; body: unknown }> = []
  const sseClients = new Set<http.ServerResponse>()
  let listDevicesCallCount = 0
  let failDeviceList = false
  let missingAuthorizedPort = false
  let injectStatusTimeoutEvent = false
  let runtimeStatus = status(network('connected'))
  let wifiNetwork = network('connected')
  let calibrationState = calibration()
  let heaterCurveState = heaterCurve()

  const setWifiNetwork = (next: NetworkSummary) => {
    wifiNetwork = next
    runtimeStatus = withStatusNetwork(runtimeStatus, next)
  }

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

      if (method === 'GET' && url.pathname === '/health') {
        sendJson(response, 200, { name: 'flux-purr-devd' })
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
                  'wifi_state_v2',
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
                'wifi_state_v2',
                'monitor',
                'flash',
              ]),
              network: wifiNetwork,
              status: withStatusNetwork(runtimeStatus, wifiNetwork),
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
          identity([
            'identity',
            'status',
            'network',
            'usb_jsonl',
            'wifi_config',
            'wifi_state_v2',
            'monitor',
          ])
        )
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/network`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        sendJson(response, 200, wifiNetwork)
        return
      }

      if (method === 'GET' && url.pathname === `/api/v1/devices/${deviceId}/status`) {
        if (missingAuthorizedPort) {
          sendMissingAuthorizedPortError(response)
          return
        }
        sendJson(response, 200, withStatusNetwork(runtimeStatus, wifiNetwork))
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
        const isClear = bodyField(body, 'op') === 'clear'
        const snapshot: NetworkSummary = {
          state: isClear ? 'disabled' : 'connecting',
          configurationGeneration: isClear ? 3 : 2,
          transitionSequence: isClear ? 3 : 2,
          failureCode: null,
          ssid: isClear ? null : String(bodyField(body, 'ssid')),
          wifiPasswordLength: isClear ? 0 : String(bodyField(body, 'password')).length,
          ip: null,
          gateway: null,
          dns: [],
          wifiRssi: null,
          lastError: null,
        }
        setWifiNetwork(snapshot)
        sendJson(response, 200, { network: snapshot })
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

      if (method === 'POST' && url.pathname === '/api/v1/firmware-bundles') {
        sendJson(response, 201, {
          artifactId: 'sha256:e2e-firmware-bundle',
          source: 'local',
          channel: 'stable',
          version: 'v0.16.4-e2e',
          sourceSha: 'e'.repeat(40),
          buildId: 'e2e-firmware-build',
          bundleSha256: 'sha256:e2e-firmware-bundle',
          size: 1234,
          layoutId: 'flux-purr/esp32-s3fh4r2/4mib/v1',
          operations: ['update', 'install_recovery'],
        })
        return
      }

      if (method === 'POST' && url.pathname === `/api/v1/devices/${deviceId}/firmware`) {
        if (bodyField(body, 'dryRun') === true) {
          sendJson(response, 200, {
            outcome: 'preflight_passed',
            approvalToken: 'approval-e2e',
            message: 'E2E firmware preflight passed.',
          })
          return
        }
        sendJson(response, 200, {
          outcome: 'verified',
          message: 'E2E firmware transaction verified.',
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
    wifiNetwork = network('connected')
    calibrationState = calibration()
    heaterCurveState = heaterCurve()
  })

  test('discovers live devd target and completes artifact dry-check through HTTP bridge', async ({
    page,
  }) => {
    const firmware = await createE2eFirmwareBundle()
    await page.route('**/firmware/releases-manifest.json', async (route) => {
      await route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify(firmware.catalog),
      })
    })
    await page.route(`**/${e2eFirmwareAssetPath}`, async (route) => {
      await route.fulfill({
        contentType: 'application/vnd.flux-purr.firmware-bundle+zip',
        body: Buffer.from(firmware.bytes),
      })
    })
    await page.goto(`/devices/${deviceId}/overview?demo=false`)

    await expectActiveDevdDeviceWorkspace(page)
    await expect(page.getByText('181.5').first()).toBeVisible()
    await expect(page.getByText('Heater 18%')).toBeVisible()
    await expect(page.getByLabel('Transport capabilities').getByText('connected')).toBeVisible()

    await page.getByRole('button', { name: '固件维护' }).click()
    await expect(page.getByRole('region', { name: '固件工作区' })).toBeVisible()
    await expect(page.getByRole('button', { name: '安装或恢复' })).toHaveAttribute(
      'aria-pressed',
      'false'
    )
    await page.getByRole('button', { name: '安装或恢复' }).click()
    await expect(page.getByRole('button', { name: '安装或恢复' })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
    const nativeTargetSelect = page.getByRole('combobox', { name: '本机固件目标' })
    await expect(nativeTargetSelect).toBeVisible()
    await nativeTargetSelect.click()
    await page.getByRole('option', { name: /flux-purr-e2e/ }).click()
    await expect(page.getByLabel('选择固件包')).not.toContainText('正在读取发布目录')
    await expect(page.getByLabel('选择固件包')).not.toContainText('发布目录不可用')
    await expect(page.getByRole('button', { name: '运行预检' })).toBeEnabled()

    await page.getByRole('button', { name: '运行预检' }).click()

    await expect(page.locator('.firmware-workbench__status[data-phase="preflight"]')).toBeVisible()
    await expect(page.getByLabel('预检进度百分比')).toHaveText('100%')
    await expect(page.getByText('devd 完整预检已通过；授权令牌五分钟内单次有效。')).toBeVisible()
    await expect
      .poll(
        () =>
          requests.filter(
            (request) => request.method === 'POST' && request.path === '/api/v1/firmware-bundles'
          ).length
      )
      .toBeGreaterThanOrEqual(1)
    await expect
      .poll(
        () =>
          requests.filter(
            (request) =>
              request.method === 'POST' && request.path === `/api/v1/devices/${deviceId}/firmware`
          ).length
      )
      .toBeGreaterThanOrEqual(1)
  })

  test('keeps the live workspace visible while devd is still reclaiming the first native probe', async ({
    page,
  }) => {
    await page.goto(`/devices/${deviceId}/overview?demo=false`)

    await expectActiveDevdDeviceWorkspace(page)
    await expect(page.getByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Choose target' })).toHaveCount(0)
    await expect(page.getByText('No known devices')).toHaveCount(0)

    await expect(page.getByLabel('Transport capabilities').getByText('connected')).toBeVisible()
  })

  test('connects a discovered USB bridge candidate through the configured devd endpoint', async ({
    page,
  }) => {
    await page.goto('/devices/new?demo=false')

    await page.getByRole('button', { name: '目标设备' }).click()
    await page.getByRole('button', { name: '添加设备' }).click()
    await page.getByRole('button', { name: /桥接/ }).click()

    const bridge = page.getByRole('region', { name: 'DEVD 桥接目标' })
    await expect(bridge.getByRole('button', { name: 'USB' })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
    await expect(bridge.getByText('flux-purr-e2e')).toBeVisible()
    await expect(bridge.getByText('/dev/cu.usbmodem-e2e')).toBeVisible()
    await expect(bridge.getByRole('button', { name: '连接' })).toBeEnabled()
    await expect(bridge.getByText('设备 ID ·')).toHaveCount(0)

    await bridge.getByRole('button', { name: '连接' }).click()
    const dialog = page.getByRole('dialog', { name: '设备已连接' })
    await expect(dialog.getByText(/已通过身份验证/)).toBeVisible()
    await dialog.getByRole('button', { name: '完成' }).click()
    await expect(page.getByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
    await expect(page.getByRole('button', { name: '目标设备' })).toHaveText(/flux-purr-e2e/)
  })

  test('preserves the chosen calibration tab and blocks calibration controls while devd is still reacquiring the lease', async ({
    page,
  }) => {
    await page.goto(`/devices/${deviceId}/overview?demo=false`)

    await page
      .getByRole('navigation', { name: '设备工作区' })
      .getByRole('link', { name: /校准/i })
      .click()
    await page.locator('.industrial-calibration-tabs__list').getByText('温度标定').click()

    const targetAdcInput = page.getByLabel('目标 ADC 输入')
    await expect(targetAdcInput).toBeVisible()
    const calibrationModeToggle = page.getByRole('switch', { name: '标定模式' })
    await expectActiveDevdDeviceWorkspace(page)
    await expect(targetAdcInput).toBeVisible()
    await expect(page.getByRole('heading', { name: '加热曲线' })).toHaveCount(0)
    await expect(calibrationModeToggle).toBeEnabled()
  })

  test('replaces a mismatched calibration URL with the running device mode', async ({ page }) => {
    runtimeStatus = {
      ...runtimeStatus,
      calibration: {
        ...calibrationRuntimeState(),
        mode: 'rtd_adc',
        ppsEnabled: true,
      },
    }

    await page.goto(`/devices/${deviceId}/calibration/heater-curve?demo=false`)

    await expect(page).toHaveURL(
      new RegExp(`/devices/${deviceId}/calibration/rtd-adc\\?demo=false$`)
    )
    await expect(page.getByRole('tab', { name: '温度标定' })).toHaveAttribute(
      'aria-current',
      'page'
    )
  })

  test('updates the RTD calibration target after heater start instead of leaving the old target latched', async ({
    page,
  }) => {
    await page.goto(`/devices/${deviceId}/overview?demo=false`)

    await page
      .getByRole('navigation', { name: '设备工作区' })
      .getByRole('link', { name: /校准/i })
      .click()
    await page.locator('.industrial-calibration-tabs__list').getByText('温度标定').click()
    const targetAdcInput = page.getByLabel('目标 ADC 输入')
    await expect(targetAdcInput).toBeVisible()
    await expectActiveDevdDeviceWorkspace(page)

    const calibrationModeToggle = page.getByRole('switch', { name: '标定模式' })
    await expect(calibrationModeToggle).toBeEnabled()
    await targetAdcInput.fill('950')
    await calibrationModeToggle.click()
    await page.waitForTimeout(700)

    await page.getByRole('switch', { name: '加热开关' }).click()
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
    await expect(targetAdcInput).toBeVisible()
    await expect(targetAdcInput).toHaveValue('980')
  })

  test('keeps dashboard target temperature writable after RTD calibration heater start', async ({
    page,
  }) => {
    await page.goto(`/devices/${deviceId}/overview?demo=false`)

    await page
      .getByRole('navigation', { name: '设备工作区' })
      .getByRole('link', { name: /校准/i })
      .click()
    await page.locator('.industrial-calibration-tabs__list').getByText('温度标定').click()
    const targetAdcInput = page.getByLabel('目标 ADC 输入')
    await expect(targetAdcInput).toBeVisible()
    await expectActiveDevdDeviceWorkspace(page)

    const calibrationModeToggle = page.getByRole('switch', { name: '标定模式' })

    await targetAdcInput.fill('950')
    await calibrationModeToggle.click()
    await page.waitForTimeout(700)
    await page.getByRole('switch', { name: '加热开关' }).click()
    await page.waitForTimeout(700)

    await page
      .getByRole('navigation', { name: '设备工作区' })
      .getByRole('link', { name: /总览/i })
      .click()
    await expect(page.getByText('请先关闭校准控制')).toBeVisible()
    await page.getByRole('button', { name: '关闭并继续' }).click({ force: true })
    await page.waitForTimeout(500)
    await page
      .getByRole('navigation', { name: '设备工作区' })
      .getByRole('link', { name: /总览/i })
      .click()
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

  test('keeps RTD slot fit after the devd calibration response is applied', async ({ page }) => {
    await page.goto(`/devices/${deviceId}/overview?demo=false`)

    await page
      .getByRole('navigation', { name: '设备工作区' })
      .getByRole('link', { name: /校准/i })
      .click()
    await page.locator('.industrial-calibration-tabs__list').getByText('温度标定').click()
    await expect(page.getByLabel('目标 ADC 输入')).toBeVisible()
    await expectActiveDevdDeviceWorkspace(page)

    const summary = page.getByLabel('当前 ADC 标定状态摘要')
    await summary.getByRole('button', { name: '编辑' }).first().click()
    const slotDialog = page.getByRole('dialog')
    await slotDialog.getByLabel('增益').fill('0.99010')
    await slotDialog.getByLabel('偏移').fill('10.9')
    await slotDialog.getByRole('button', { name: '保存' }).click()

    await expect(summary).toContainText('槽位 A')
    await expect(summary).toContainText('0.99010x')
    await expect(summary).toContainText('10.9mV')
  })

  test('sends runtime commands through the active devd lease', async ({ page }) => {
    await page.goto(`/devices/${deviceId}/overview?demo=false`)

    await expect(page.getByRole('button', { name: '目标设备' })).toContainText('DEVD')
    await expect(page.getByText('运行时已同步')).toBeVisible()

    await page.getByRole('link', { name: /总览/i }).click()
    await page.getByLabel('Dashboard target temperature').fill('235')
    await expect(page.getByText('Target updated')).toBeVisible()

    await page.getByRole('link', { name: /设置/i }).click()
    await page.getByRole('button', { name: 'OFF' }).click()
    await expect(page.getByText('Fan policy updated', { exact: true })).toBeVisible()

    await page.getByRole('link', { name: /总览/i }).click()
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

  test('writes and clears WiFi settings through the active devd lease', async ({ page }) => {
    await page.goto(`/devices/${deviceId}/overview?demo=false`)

    await expectActiveDevdDeviceWorkspace(page)
    await page
      .getByRole('navigation', { name: '设备工作区' })
      .getByRole('link', { name: /设置/i })
      .click()

    const wifiSettings = page.getByLabel('WiFi 设置')
    await expect(wifiSettings).toBeVisible()
    await wifiSettings.getByLabel('WiFi 名称').fill('FluxPurr-E2E')
    await wifiSettings.getByLabel('密码').fill('test-wifi-pass')
    await wifiSettings.getByRole('button', { name: '保存并连接' }).click()

    await expect(page.getByText('已提交，正在等待设备连接。')).toBeVisible()
    await expect.poll(() => wifiRequests().length).toBe(1)
    expect(wifiRequests()[0]?.body).toEqual(
      expect.objectContaining({
        leaseId: 'lease-e2e',
        op: 'set',
        ssid: 'FluxPurr-E2E',
        password: 'test-wifi-pass',
      })
    )
    await expect(page.getByText('test-wifi-pass')).toHaveCount(0)

    setWifiNetwork(network('connected', 2, 3))
    await expect(page.getByText('WiFi 已连接。')).toBeVisible({ timeout: 5_000 })

    await wifiSettings.getByRole('button', { name: '清除 WiFi' }).click()
    await wifiSettings.getByRole('button', { name: '确认清除' }).click()

    await expect(page.getByText('已清除设备中的 WiFi 设置。')).toBeVisible()
    await expect.poll(() => wifiRequests().length).toBe(2)
    expect(wifiRequests()[1]?.body).toEqual(
      expect.objectContaining({ leaseId: 'lease-e2e', op: 'clear' })
    )
  })

  test('keeps the live devd workspace visible across repeated reloads', async ({ page }) => {
    await page.goto(`/devices/${deviceId}/overview?demo=false`)

    for (let reloadIndex = 0; reloadIndex < 3; reloadIndex += 1) {
      await expectActiveDevdDeviceWorkspace(page)
      await expect(page.getByRole('heading', { name: 'Choose target' })).toHaveCount(0)
      await expect(page.getByText('No known devices')).toHaveCount(0)
      await expect(page.getByText('Failed to fetch')).toHaveCount(0)

      await page.reload()
      await expectActiveDevdDeviceWorkspace(page)
      await expect(page.getByRole('heading', { name: 'Choose target' })).toHaveCount(0)
      await expect(page.getByText('No known devices')).toHaveCount(0)
      await expect(page.getByText('Failed to fetch')).toHaveCount(0)
      await expect(page.getByText('Lease conflict')).toHaveCount(0)
      await expect(page.getByText('lease_conflict')).toHaveCount(0)
    }
  })

  test('keeps the identity URL and offers recovery when the device list refresh fails', async ({
    page,
  }) => {
    injectStatusTimeoutEvent = true
    failDeviceList = true

    await page.goto(`/devices/${deviceId}/overview?demo=false`)

    await expect(page).toHaveURL(new RegExp(`/devices/${deviceId}/overview\\?demo=false$`))
    await expect(page.getByRole('heading', { name: 'Choose target' })).toBeVisible()
    await expect(page.getByRole('status').getByText('连接恢复')).toBeVisible()
    await expect(page.getByRole('button', { name: '重试恢复' })).toBeVisible()
    await expect(
      page.getByRole('region', { name: 'Add device' }).getByRole('button', { name: /Web Serial/ })
    ).toBeVisible()
    injectStatusTimeoutEvent = false
  })

  test('surfaces the missing authorized serial port instead of falling back to the empty chooser', async ({
    page,
  }) => {
    missingAuthorizedPort = true

    await page.goto(`/devices/${deviceId}/overview?demo=false`)

    const targetRegion = page.getByRole('region', { name: '当前目标' })
    await expect(page.getByRole('button', { name: '目标设备' })).toContainText('DEVD')
    await expect(targetRegion).toContainText('DEVD')
    await expect(page.getByRole('navigation', { name: '设备工作区' })).toBeVisible()
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

  if (!request.headers['content-type']?.includes('application/json')) {
    return null
  }

  return JSON.parse(Buffer.concat(chunks).toString('utf8'))
}

async function expectActiveDevdDeviceWorkspace(page: Page) {
  await expect(page.getByRole('button', { name: '目标设备' })).toContainText('DEVD')
  await expect(page.getByRole('navigation', { name: '设备工作区' })).toBeVisible()
  await expect(page.getByText('运行时已同步')).toBeVisible()
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
    apiVersion: '2026-05-29',
    protocolVersion: 'flux-purr.usb.v1',
    hostname: 'flux-purr-e2e',
    capabilities,
  }
}

function network(
  state: NetworkSummary['state'],
  configurationGeneration = 1,
  transitionSequence = 1
): NetworkSummary {
  return {
    state,
    configurationGeneration,
    transitionSequence,
    failureCode: null,
    ssid: state === 'connected' ? 'FluxPurr-E2E' : null,
    wifiPasswordLength: state === 'connected' ? 11 : 0,
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
  return {
    rtdAdc: {
      samples: [
        { observedMv: 1123, expectedMv: 980, referenceTempC: 25, targetAdcMv: 980 },
        { observedMv: 1188, expectedMv: 1120, referenceTempC: 60, targetAdcMv: 1120 },
        null,
        null,
        null,
        null,
        null,
        null,
      ],
      fittedFit: createCalibrationFit(
        [
          { observedMv: 1123, expectedMv: 980, referenceTempC: 25, targetAdcMv: 980 },
          { observedMv: 1188, expectedMv: 1120, referenceTempC: 60, targetAdcMv: 1120 },
          null,
          null,
          null,
          null,
          null,
          null,
        ],
        'rtd_adc'
      ),
      slots: {
        a: { gain: 1, offsetMv: 0 },
        b: { gain: 1, offsetMv: 0 },
      },
      activeSlot: 'a',
    },
    vinAdc: {
      samples: [
        { observedMv: 1678, expectedMv: 20000, referenceVinMv: 20000 },
        null,
        null,
        null,
        null,
        null,
        null,
        null,
      ],
      fittedFit: createCalibrationFit(
        [
          { observedMv: 1678, expectedMv: 20000, referenceVinMv: 20000 },
          null,
          null,
          null,
          null,
          null,
          null,
          null,
        ],
        'vin_adc'
      ),
      slots: {
        a: { gain: 1, offsetMv: 0 },
        b: { gain: 1, offsetMv: 0 },
      },
      activeSlot: 'a',
    },
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
    const stateValue = calibrationStateValue(bodyField(body, 'state'))
    if (!stateValue) {
      return cloneCalibrationState(current)
    }
    return normalizeCalibrationState(stateValue)
  }

  const channel = calibrationChannelValue(bodyField(body, 'channel'))
  if (!channel) {
    return cloneCalibrationState(current)
  }

  const next = cloneCalibrationState(current)
  const channelState = channel === 'rtd_adc' ? next.rtdAdc : next.vinAdc
  const samples = channelState.samples

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
    const referenceTempC = numberOrFallback(bodyField(body, 'referenceTempC'), null)
    const referenceVinMv = numberOrFallback(
      bodyField(body, 'referenceVinMv'),
      currentStatus.voltageMv
    )
    const targetAdcMv = numberOrFallback(
      bodyField(body, 'targetAdcMv'),
      currentStatus.calibration.targetAdcMv ?? currentStatus.rtdRawAdcMv
    )
    const nextSample =
      channel === 'rtd_adc'
        ? {
            observedMv: currentStatus.rtdRawAdcMv ?? 0,
            expectedMv: targetAdcMv ?? 0,
            ...(referenceTempC != null ? { referenceTempC } : {}),
            ...(targetAdcMv != null ? { targetAdcMv } : {}),
          }
        : {
            observedMv: currentStatus.vinRawAdcMv ?? 0,
            expectedMv: vinAdcMvForInput(referenceVinMv ?? 0),
            ...(referenceVinMv != null ? { referenceVinMv } : {}),
          }
    const emptyIndex = samples.findIndex((sample) => sample == null)
    samples[emptyIndex === -1 ? samples.length - 1 : emptyIndex] = nextSample
  } else if (op === 'set_active_slot') {
    const slot = bodyField(body, 'slot')
    if (slot === 'a' || slot === 'b') {
      channelState.activeSlot = slot
    }
  } else if (op === 'set_slot_fit') {
    const slot = bodyField(body, 'slot')
    const fit = calibrationSlotFitValue(bodyField(body, 'fit'))
    if ((slot === 'a' || slot === 'b') && fit) {
      channelState.slots[slot] = fit
    }
  }

  return normalizeCalibrationState(next)
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
            bodyField(body, 'kind') === 'thermal_plant_auto' ||
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

function createCalibrationFit(
  samples: Array<Record<string, unknown> | null>,
  channel: CalibrationChannel
) {
  const custom = samples.filter(
    (sample): sample is { observedMv: number; expectedMv: number } =>
      sample != null &&
      Number.isFinite(sample.observedMv) &&
      Number.isFinite(sample.expectedMv) &&
      (channel !== 'rtd_adc' ||
        (Number.isFinite(sample.referenceTempC) && Number.isFinite(sample.targetAdcMv)))
  )
  if (custom.length === 0) {
    return {
      gain: 1,
      offsetMv: 0,
      sampleCount: 0,
    }
  }
  if (custom.length === 1) {
    return {
      gain: 1,
      offsetMv: custom[0].expectedMv - custom[0].observedMv,
      sampleCount: 1,
    }
  }
  const points = custom
  const n = points.length
  const sumX = points.reduce((sum, sample) => sum + sample.observedMv, 0)
  const sumY = points.reduce((sum, sample) => sum + sample.expectedMv, 0)
  const sumXX = points.reduce((sum, sample) => sum + sample.observedMv * sample.observedMv, 0)
  const sumXY = points.reduce((sum, sample) => sum + sample.observedMv * sample.expectedMv, 0)
  const denominator = n * sumXX - sumX * sumX
  const gain = Math.abs(denominator) < Number.EPSILON ? 1 : (n * sumXY - sumX * sumY) / denominator
  const offsetMv =
    Math.abs(denominator) < Number.EPSILON ? (sumY - sumX) / n : (sumY - gain * sumX) / n
  return {
    gain,
    offsetMv,
    sampleCount: custom.length,
  }
}

function cloneCalibrationState(current: CalibrationState): CalibrationState {
  return {
    rtdAdc: {
      samples: current.rtdAdc.samples.map((sample) => (sample ? { ...sample } : null)),
      fittedFit: { ...current.rtdAdc.fittedFit },
      slots: {
        a: { ...current.rtdAdc.slots.a },
        b: { ...current.rtdAdc.slots.b },
      },
      activeSlot: current.rtdAdc.activeSlot,
    },
    vinAdc: {
      samples: current.vinAdc.samples.map((sample) => (sample ? { ...sample } : null)),
      fittedFit: { ...current.vinAdc.fittedFit },
      slots: {
        a: { ...current.vinAdc.slots.a },
        b: { ...current.vinAdc.slots.b },
      },
      activeSlot: current.vinAdc.activeSlot,
    },
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

function normalizeCalibrationSample(value: unknown) {
  const record = recordValue(value)
  if (!record) {
    return null
  }
  const observedMv = numberOrFallback(record.observedMv, null)
  const expectedMv = numberOrFallback(record.expectedMv, null)
  if (observedMv == null || expectedMv == null) {
    return null
  }
  const referenceTempC = numberOrFallback(record.referenceTempC, null)
  const targetAdcMv = numberOrFallback(record.targetAdcMv, null)
  const referenceVinMv = numberOrFallback(record.referenceVinMv, null)
  return {
    observedMv,
    expectedMv,
    ...(referenceTempC != null ? { referenceTempC } : {}),
    ...(targetAdcMv != null ? { targetAdcMv } : {}),
    ...(referenceVinMv != null ? { referenceVinMv } : {}),
  }
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

function calibrationStateValue(value: unknown): CalibrationState | null {
  const record = recordValue(value)
  if (!record) {
    return null
  }
  const rtdAdc = calibrationChannelStateValue(record.rtdAdc)
  const vinAdc = calibrationChannelStateValue(record.vinAdc)
  if (!rtdAdc || !vinAdc) {
    return null
  }
  return {
    rtdAdc,
    vinAdc,
  }
}

function calibrationChannelStateValue(value: unknown) {
  const record = recordValue(value)
  if (!record || !Array.isArray(record.samples)) {
    return null
  }
  const fittedFit = calibrationFitValue(record.fittedFit)
  const slots = calibrationSlotSetValue(record.slots)
  const activeSlot = calibrationSlotIdValue(record.activeSlot)
  if (!fittedFit || !slots || !activeSlot) {
    return null
  }
  return {
    samples: record.samples.map(normalizeCalibrationSample),
    fittedFit,
    slots,
    activeSlot,
  }
}

function calibrationFitValue(value: unknown): CalibrationFit | null {
  const record = recordValue(value)
  if (!record) {
    return null
  }
  const gain = numberOrFallback(record.gain, null)
  const offsetMv = numberOrFallback(record.offsetMv, null)
  const sampleCount = numberOrFallback(record.sampleCount, null)
  if (gain == null || offsetMv == null || sampleCount == null) {
    return null
  }
  return {
    gain,
    offsetMv,
    sampleCount,
  }
}

function calibrationSlotFitValue(value: unknown): CalibrationSlotFit | null {
  const record = recordValue(value)
  if (!record) {
    return null
  }
  const gain = numberOrFallback(record.gain, null)
  const offsetMv = numberOrFallback(record.offsetMv, null)
  if (gain == null || offsetMv == null) {
    return null
  }
  return {
    gain,
    offsetMv,
  }
}

function calibrationSlotSetValue(value: unknown) {
  const record = recordValue(value)
  if (!record) {
    return null
  }
  const a = calibrationSlotFitValue(record.a)
  const b = calibrationSlotFitValue(record.b)
  if (!a || !b) {
    return null
  }
  return { a, b }
}

function calibrationSlotIdValue(value: unknown) {
  return value === 'a' || value === 'b' ? value : null
}

function normalizeCalibrationState(current: CalibrationState): CalibrationState {
  return {
    rtdAdc: normalizeCalibrationChannelState(current.rtdAdc, 'rtd_adc'),
    vinAdc: normalizeCalibrationChannelState(current.vinAdc, 'vin_adc'),
  }
}

function normalizeCalibrationChannelState(
  current: CalibrationState['rtdAdc'] | CalibrationState['vinAdc'],
  channel: CalibrationChannel
) {
  const samples = normalizeCalibrationChannelSamples(current.samples)
  return {
    samples,
    fittedFit: createCalibrationFit(samples, channel),
    slots: {
      a: normalizeCalibrationSlotFit(current.slots.a),
      b: normalizeCalibrationSlotFit(current.slots.b),
    },
    activeSlot: calibrationSlotIdValue(current.activeSlot) ?? 'a',
  }
}

function normalizeCalibrationChannelSamples(
  samples: Array<Record<string, unknown> | null>
): Array<Record<string, unknown> | null> {
  const compact = samples
    .map(normalizeCalibrationSample)
    .filter(
      (sample): sample is NonNullable<ReturnType<typeof normalizeCalibrationSample>> =>
        sample != null
    )
  return Array.from({ length: 8 }, (_, index) => compact[index] ?? null)
}

function normalizeCalibrationSlotFit(fit: CalibrationSlotFit): CalibrationSlotFit {
  return {
    gain: Number.isFinite(fit.gain) ? fit.gain : 1,
    offsetMv: Number.isFinite(fit.offsetMv) ? fit.offsetMv : 0,
  }
}

function vinAdcMvForInput(inputMv: number) {
  return Math.round((inputMv * 5100) / (56_000 + 5100))
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
