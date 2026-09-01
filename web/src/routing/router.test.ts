import { createMemoryHistory, createRouter, isRedirect } from '@tanstack/react-router'
import { describe, expect, it } from 'vitest'
import { Route as SettingsRoute } from '@/routes/_console.devices.$deviceId.settings'
import { routeTree } from '@/routeTree.gen'

function createTestRouter(initialEntry: string) {
  return createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [initialEntry] }),
  })
}

describe('TanStack route tree', () => {
  it.each([
    '/devices/device-1/overview?demo=true',
    '/devices/device-1/settings/presets?demo=true',
    '/devices/device-1/settings/fan?demo=true',
    '/devices/device-1/settings/wifi?demo=true',
    '/devices/device-1/update?demo=true',
    '/devices/device-1/calibration/heater-curve?demo=true',
    '/devices/device-1/calibration/rtd-adc?demo=true',
    '/devices/device-1/calibration/vin-adc?demo=true',
  ])('directly matches canonical leaf %s', async (entry) => {
    const router = createTestRouter(entry)
    await router.load()

    expect(router.state.status).toBe('idle')
    expect(router.state.location.pathname).toBe(entry.split('?')[0])
    expect(router.state.matches.at(-1)?.status).toBe('success')
  })

  it('redirects the legacy settings entry to presets', async () => {
    const beforeLoad = SettingsRoute.options.beforeLoad
    expect(beforeLoad).toBeDefined()
    try {
      beforeLoad?.({
        location: { pathname: '/devices/device-1/settings' },
        params: { deviceId: 'device-1' },
        search: { demo: true },
      } as never)
      throw new Error('expected redirect')
    } catch (error) {
      expect(isRedirect(error)).toBe(true)
      if (!isRedirect(error)) return
      expect(error.options).toMatchObject({
        to: '/devices/$deviceId/settings/presets',
        params: { deviceId: 'device-1' },
        search: { demo: true },
        replace: true,
      })
    }
  })
})
