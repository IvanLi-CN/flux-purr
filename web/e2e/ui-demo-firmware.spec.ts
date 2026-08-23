import { expect, test } from '@playwright/test'

type FirmwareDemoTelemetry = {
  fetch: number
  getPorts: number
  requestPort: number
  eventSource: number
}

declare global {
  interface Window {
    __firmwareDemoTelemetry?: FirmwareDemoTelemetry
  }
}

type DemoPage = Parameters<typeof test>[0]['page']

test.describe('firmware workspace ui demo', () => {
  test.setTimeout(90_000)

  test('keeps the development firmware evidence route fully mocked', async ({ page }, testInfo) => {
    const crossOriginRequests = await installDemoGuards(page, testInfo.project.use.baseURL)
    await runMockFirmwareTransaction(page, '/?uiDemo=firmware-workspace&workspace=firmware')
    expect(crossOriginRequests).toEqual([])
  })

  test('keeps the regular demo firmware route fully mocked', async ({ page }, testInfo) => {
    const crossOriginRequests = await installDemoGuards(page, testInfo.project.use.baseURL)
    await runMockFirmwareTransaction(page, '/devices/fp-lab-01/update?demo=true')
    expect(crossOriginRequests).toEqual([])
  })
})

async function installDemoGuards(page: DemoPage, baseURL: string | undefined) {
  if (!baseURL) throw new Error('The UI demo test requires a configured base URL.')
  const expectedOrigin = new URL(baseURL).origin
  const crossOriginRequests: string[] = []
  page.on('request', (request) => {
    const url = new URL(request.url())
    if ((url.protocol === 'http:' || url.protocol === 'https:') && url.origin !== expectedOrigin) {
      crossOriginRequests.push(request.url())
    }
  })

  await page.addInitScript(() => {
    const telemetry = { fetch: 0, getPorts: 0, requestPort: 0, eventSource: 0 }
    Object.defineProperty(window, '__firmwareDemoTelemetry', {
      configurable: true,
      value: telemetry,
    })
    Object.defineProperty(navigator, 'serial', {
      configurable: true,
      value: {
        getPorts: async () => {
          telemetry.getPorts += 1
          return []
        },
        requestPort: async () => {
          telemetry.requestPort += 1
          throw new Error('The ui demo must not request a real serial port.')
        },
      },
    })
    const nativeFetch = window.fetch
    window.fetch = ((...args: Parameters<typeof fetch>) => {
      telemetry.fetch += 1
      return nativeFetch(...args)
    }) as typeof fetch
    Object.defineProperty(window, 'EventSource', {
      configurable: true,
      value: function BlockedDemoEventSource() {
        telemetry.eventSource += 1
        throw new Error('The ui demo must not open an EventSource.')
      },
    })
  })

  return crossOriginRequests
}

async function runMockFirmwareTransaction(page: DemoPage, url: string) {
  await page.goto(url)
  await expect(page.getByRole('heading', { name: '热控工作台' })).toBeVisible({
    timeout: 30_000,
  })
  await expect(page.getByText('演示模拟 Browser USB ROM')).toBeVisible()

  const beforeInteraction = await demoTelemetry(page)
  expect(beforeInteraction).toEqual({ fetch: 0, getPorts: 0, requestPort: 0, eventSource: 0 })
  await page.getByRole('button', { name: '选择固件包' }).click()
  await page.getByRole('tab', { name: '演示本地包' }).click()
  await expect(page.locator('input[type="file"]')).toHaveCount(0)
  await page.getByRole('button', { name: '采用演示本地包' }).click()

  await page.getByRole('button', { name: '运行预检' }).click()
  await expect(
    page.getByText('演示预检已通过；未请求浏览器 USB、devd、网络或真实固件文件。')
  ).toBeVisible()
  await expect(page.locator('.firmware-workbench__status[data-phase="preflight"]')).toBeVisible()
  await expect(page.getByLabel('预检进度百分比')).toHaveText('100%')
  await expect(page.locator('.firmware-workbench__status[data-phase="execution"]')).toHaveCount(0)
  await expect(page.getByRole('button', { name: '开始更新' })).toBeEnabled()

  await page.getByRole('button', { name: '开始更新' }).click()
  await expect(page.locator('.firmware-workbench__status[data-phase="execution"]')).toBeVisible()
  await expect(page.getByLabel('更新进度百分比')).not.toHaveText('100%')
  await expect(
    page.getByText('演示固件事务已验证；未连接、复位、擦除或写入任何设备。')
  ).toBeVisible()
  await expect(page.getByLabel('更新进度百分比')).toHaveText('100%')
  await expect(page.getByText('模拟浏览器 USB 已确认')).toBeVisible()

  expect(await demoTelemetry(page)).toEqual(beforeInteraction)
}

async function demoTelemetry(page: DemoPage) {
  return page.evaluate(() => window.__firmwareDemoTelemetry)
}
