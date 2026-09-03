import { describe, expect, it } from 'vitest'
import { consoleRoutePath, parseConsoleRoute } from './console-route'

describe('console route contract', () => {
  it.each([
    ['/devices/device%2Falpha/overview', 'dashboard', undefined, undefined],
    ['/devices/device-1/settings', 'settings', undefined, 'presets'],
    ['/devices/device-1/settings/presets', 'settings', undefined, 'presets'],
    ['/devices/device-1/settings/fan', 'settings', undefined, 'fan'],
    ['/devices/device-1/settings/wifi', 'settings', undefined, 'wifi'],
    ['/devices/device-1/update', 'update', undefined, undefined],
    ['/devices/device-1/calibration/heater-curve', 'calibration', 'heater_curve', undefined],
    ['/devices/device-1/calibration/rtd-adc', 'calibration', 'rtd_adc', undefined],
    ['/devices/device-1/calibration/vin-adc', 'calibration', 'vin_adc', undefined],
    ['/devices/device-1/calibration/thermal-tuning', 'calibration', 'thermal_tuning', undefined],
  ])('parses canonical leaf %s', (pathname, view, calibrationTab, settingsTab) => {
    const route = parseConsoleRoute(pathname)
    expect(route).toMatchObject({ kind: 'device', view })
    if (calibrationTab) {
      expect(route).toMatchObject({ calibrationTab })
    }
    if (settingsTab) {
      expect(route).toMatchObject({ settingsTab })
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

  it('round-trips settings tabs and canonicalizes the legacy settings URL', () => {
    expect(parseConsoleRoute('/devices/device-1/settings')).toMatchObject({
      view: 'settings',
      settingsTab: 'presets',
    })
    expect(
      consoleRoutePath({
        kind: 'device',
        deviceId: 'device-1',
        view: 'settings',
        settingsTab: 'wifi',
      })
    ).toBe('/devices/device-1/settings/wifi')
  })

  it('rejects non-canonical and structurally invalid paths', () => {
    expect(parseConsoleRoute('/devices/device-1')).toBeNull()
    expect(parseConsoleRoute('/devices/device-1/calibration')).toBeNull()
    expect(parseConsoleRoute('/devices/device-1/calibration/unknown')).toBeNull()
    expect(parseConsoleRoute('/devices/device-1/settings/unknown')).toBeNull()
    expect(parseConsoleRoute('/unknown')).toBeNull()
  })
})
