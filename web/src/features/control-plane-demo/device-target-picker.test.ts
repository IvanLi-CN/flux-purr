import { describe, expect, it } from 'vitest'
import {
  deviceIdentityId,
  isDeviceConnectionAvailable,
  mergeDeviceChoices,
  preferredDeviceConnection,
} from './device-target-picker'
import type { DeviceTarget } from './types'

function target(overrides: Partial<DeviceTarget>): DeviceTarget {
  return {
    id: 'target-1',
    identityId: 'device-1',
    alias: 'flux-purr-device-1',
    location: 'unknown',
    transport: 'devd',
    bridgeTransport: 'usb',
    severity: 'nominal',
    baseUrl: 'devd://target-1',
    firmware: '0.1.0',
    buildId: 'build-1',
    uptime: '1m',
    boardTempC: 30,
    currentTempC: 30,
    targetTempC: 50,
    voltageMv: 12000,
    currentMa: 500,
    pdRequestMv: 12000,
    pdContractMv: 12000,
    pdState: 'ready',
    calibration: {} as DeviceTarget['calibration'],
    heaterEnabled: false,
    heaterOutputPercent: 0,
    activeCoolingEnabled: false,
    fanState: 'OFF',
    wifiRssi: null,
    capabilities: [],
    leaseState: 'active',
    ...overrides,
  }
}

describe('device target picker', () => {
  it('uses the firmware identity as the stable merge key', () => {
    expect(deviceIdentityId(target({ id: 'lan-device-1', identityId: 'device-1' }))).toBe(
      'device-1'
    )
    expect(deviceIdentityId(target({ id: 'web-serial-device-1', identityId: undefined }))).toBe(
      'device-1'
    )
  })

  it('renders one device card while preserving distinct bridge sources', () => {
    const choices = mergeDeviceChoices([
      target({ id: 'native-device-1', transport: 'devd', bridgeTransport: 'usb' }),
      target({
        id: 'lan-device-1',
        transport: 'wifi',
        baseUrl: 'http://192.168.1.42',
        location: '192.168.1.42',
      }),
      target({
        id: 'web-serial-device-1',
        transport: 'serial',
        baseUrl: 'webserial://selected',
        location: 'Browser Web Serial',
      }),
      target({
        id: 'bridge-lan-device-1',
        transport: 'devd',
        bridgeTransport: 'wifi',
        baseUrl: 'devd://lan-device-1',
      }),
    ])

    expect(choices).toHaveLength(1)
    expect(choices[0].identityId).toBe('device-1')
    expect(choices[0].connections.map((connection) => connection.kind)).toEqual([
      'wifi',
      'web-serial',
      'bridge',
      'bridge',
    ])
    expect(choices[0].connections).toHaveLength(4)
    expect(choices[0].connections.map((connection) => connection.label)).toEqual([
      'WiFi / LAN',
      'Web Serial',
      '桥接',
      '桥接',
    ])
    expect(choices[0].connections[2].detail).toContain('USB')
    expect(choices[0].connections[3].detail).toContain('WiFi / LAN')
    expect(preferredDeviceConnection(choices[0], 'bridge', 'bridge-lan-device-1')?.target.id).toBe(
      'bridge-lan-device-1'
    )
  })

  it('does not expose mock connections in live mode', () => {
    const choices = mergeDeviceChoices([target({ transport: 'mock', id: 'mock-device-1' })], {
      allowDemoControls: false,
    })
    expect(choices).toEqual([])
  })

  it('does not expose a missing authorized serial placeholder as a connection', () => {
    const missingTarget = target({
      id: 'serial-_dev_cu.usbmodem21221401',
      identityId: 'serial-_dev_cu.usbmodem21221401',
      alias: 'serial-_dev_cu.usbmodem21221401',
      location: '/dev/cu.usbmodem21221401',
      severity: 'warning',
      connectionAvailable: false,
      leaseState: 'none',
      buildId: 'native-serial-placeholder',
      transportIssue: 'Authorized serial port is missing.',
    })
    const choices = mergeDeviceChoices([missingTarget])

    expect(isDeviceConnectionAvailable(missingTarget)).toBe(false)
    expect(choices).toEqual([])
  })

  it('keeps the healthiest target when a transport publishes duplicate records', () => {
    const choices = mergeDeviceChoices([
      target({
        id: 'lan-stale-device-1',
        transport: 'wifi',
        severity: 'warning',
        leaseState: 'expired',
        location: '192.168.1.40',
      }),
      target({
        id: 'lan-live-device-1',
        transport: 'wifi',
        severity: 'nominal',
        leaseState: 'active',
        location: '192.168.1.42',
      }),
    ])

    expect(choices[0].connections).toHaveLength(1)
    expect(choices[0].connections[0].target.id).toBe('lan-live-device-1')
    expect(choices[0].primary.id).toBe('lan-live-device-1')
  })

  it('prefers the last successful transport before the active healthy fallback', () => {
    const choice = mergeDeviceChoices([
      target({ id: 'bridge-device-1', transport: 'devd', leaseState: 'active' }),
      target({
        id: 'lan-device-1',
        transport: 'wifi',
        baseUrl: 'http://192.168.1.42',
        leaseState: 'none',
      }),
    ])[0]

    expect(preferredDeviceConnection(choice, 'wifi')?.target.id).toBe('lan-device-1')
    expect(preferredDeviceConnection(choice)?.target.id).toBe('bridge-device-1')
  })
})
