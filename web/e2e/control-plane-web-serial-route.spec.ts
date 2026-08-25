import { expect, test } from '@playwright/test'

const deviceId = 'serial-web-e2e'
const routePreferencesKey = 'flux-purr.routePreferences.v1'
const knownDevicesKey = 'flux-purr:known-web-serial-devices:v1'

test.describe('control plane Web Serial route recovery', () => {
  test('does not probe devd when the remembered route is direct Web Serial', async ({ page }) => {
    const devdRequests: string[] = []
    await page.addInitScript(
      ({ routePreferencesKey: preferencesKey, knownDevicesKey: devicesKey, deviceId: id }) => {
        window.localStorage.setItem(
          preferencesKey,
          JSON.stringify({
            lastDeviceByVariant: { live: id },
            transportByIdentity: { [id]: 'web-serial' },
          })
        )
        window.localStorage.setItem(
          devicesKey,
          JSON.stringify([
            {
              deviceId: id,
              hostname: 'flux-purr-web-e2e',
              firmwareVersion: 'test',
              buildId: 'test',
            },
          ])
        )

        let requestPortCalls = 0
        Object.defineProperty(window, '__fluxPurrWebSerialProbe', {
          configurable: true,
          value: {
            get requestPortCalls() {
              return requestPortCalls
            },
          },
        })
        Object.defineProperty(navigator, 'serial', {
          configurable: true,
          value: {
            getPorts: async () => [],
            requestPort: async () => {
              requestPortCalls += 1
              throw new Error('unexpected Web Serial chooser')
            },
          },
        })
      },
      { routePreferencesKey, knownDevicesKey, deviceId }
    )
    page.on('request', (request) => {
      if (request.url().includes('/api/v1/')) {
        devdRequests.push(request.url())
      }
    })

    await page.goto(`/devices/${deviceId}/overview?demo=false`)
    await expect(page).toHaveURL(new RegExp(`/devices/${deviceId}/overview\\?demo=false$`))
    await expect(page.getByRole('heading', { name: 'Choose target' })).toBeVisible()
    await expect(page.getByRole('status').getByText('连接恢复')).toBeVisible()
    await expect(page.getByRole('button', { name: '重试恢复' })).toBeVisible()
    expect(await page.getByRole('button', { name: /Web Serial/ }).count()).toBeGreaterThan(0)
    await expect(page.getByText('Web Serial unavailable')).toBeVisible()
    expect(await page.getByText('目标设备暂不可用', { exact: true }).count()).toBe(0)
    await page.waitForTimeout(1_000)

    expect(devdRequests).toEqual([])
    expect(
      await page.evaluate(
        () =>
          (window as Window & { __fluxPurrWebSerialProbe?: { requestPortCalls: number } })
            .__fluxPurrWebSerialProbe?.requestPortCalls
      )
    ).toBe(0)
  })
})
