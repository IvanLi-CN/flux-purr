import { createMemoryHistory, createRouter } from '@tanstack/react-router'
import { describe, expect, it } from 'vitest'
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
    '/devices/device-1/settings?demo=true',
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
})
