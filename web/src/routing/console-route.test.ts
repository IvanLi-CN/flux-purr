import { describe, expect, it } from 'vitest'
import { consoleRoutePath, parseConsoleRoute } from './console-route'

describe('console route contract', () => {
  it.each([
    ['/devices/device%2Falpha/overview', 'dashboard', undefined],
    ['/devices/device-1/settings', 'settings', undefined],
    ['/devices/device-1/update', 'update', undefined],
    ['/devices/device-1/calibration/heater-curve', 'calibration', 'heater_curve'],
    ['/devices/device-1/calibration/rtd-adc', 'calibration', 'rtd_adc'],
    ['/devices/device-1/calibration/vin-adc', 'calibration', 'vin_adc'],
  ])('parses canonical leaf %s', (pathname, view, calibrationTab) => {
    const route = parseConsoleRoute(pathname)
    expect(route).toMatchObject({ kind: 'device', view })
    if (calibrationTab) {
      expect(route).toMatchObject({ calibrationTab })
    }
  })

  it('round-trips an encoded stable identity', () => {
    const state = {
      kind: 'device' as const,
      deviceId: 'device/alpha',
      view: 'calibration' as const,
      calibrationTab: 'rtd_adc' as const,
    }
    expect(parseConsoleRoute(consoleRoutePath(state))).toEqual(state)
  })

  it('rejects non-canonical and structurally invalid paths', () => {
    expect(parseConsoleRoute('/devices/device-1')).toBeNull()
    expect(parseConsoleRoute('/devices/device-1/calibration')).toBeNull()
    expect(parseConsoleRoute('/devices/device-1/calibration/unknown')).toBeNull()
    expect(parseConsoleRoute('/unknown')).toBeNull()
  })
})
