import { describe, expect, it } from 'vitest'
import { resolveWifiSettingsAccess } from './wifi-settings-access'

const base = {
  capabilities: ['identity', 'status', 'wifi_config', 'wifi_state_v2'],
  severity: 'nominal' as const,
  connectionAvailable: true,
  networkState: 'connected' as const,
}

describe('resolveWifiSettingsAccess', () => {
  it('keeps an otherwise writable USB route read-only while firmware tuning is active', () => {
    expect(
      resolveWifiSettingsAccess({
        ...base,
        transport: 'devd',
        bridgeTransport: 'usb',
        leaseId: 'lease-1',
        leaseState: 'active',
        thermalTuningActive: true,
      })
    ).toEqual({
      mode: 'read-only',
      reason: '设备正在执行固件热控调优；仅允许读取和停止调优。',
    })
  })

  it('allows active native USB bridge writes', () => {
    expect(
      resolveWifiSettingsAccess({
        ...base,
        transport: 'devd',
        bridgeTransport: 'usb',
        leaseId: 'lease-1',
        leaseState: 'active',
      })
    ).toEqual({ mode: 'read-write' })
  })

  it('allows connected Web Serial writes', () => {
    expect(
      resolveWifiSettingsAccess({
        ...base,
        transport: 'serial',
        leaseState: 'active',
      })
    ).toEqual({ mode: 'read-write' })
  })

  it('keeps an authorized USB route read-only after a transport failure', () => {
    expect(
      resolveWifiSettingsAccess({
        ...base,
        transport: 'serial',
        leaseState: 'active',
        transportIssue: 'Timed out waiting for a matching USB JSONL response.',
      })
    ).toEqual({
      mode: 'read-only',
      reason: 'Timed out waiting for a matching USB JSONL response.',
    })
  })

  it('keeps direct LAN and devd LAN bridge read-only', () => {
    expect(resolveWifiSettingsAccess({ ...base, transport: 'wifi', leaseState: 'none' }).mode).toBe(
      'read-only'
    )
    expect(
      resolveWifiSettingsAccess({
        ...base,
        transport: 'devd',
        bridgeTransport: 'wifi',
        leaseId: 'lease-1',
        leaseState: 'active',
      }).reason
    ).toContain('WiFi / LAN')
  })

  it('keeps legacy USB firmware visible but read-only', () => {
    expect(
      resolveWifiSettingsAccess({
        ...base,
        capabilities: ['identity', 'status', 'wifi_config'],
        transport: 'devd',
        bridgeTransport: 'usb',
        leaseId: 'lease-1',
        leaseState: 'active',
      })
    ).toMatchObject({ mode: 'read-only' })
  })

  it('hides targets without any WiFi capability', () => {
    expect(
      resolveWifiSettingsAccess({
        ...base,
        capabilities: ['identity', 'status'],
        transport: 'mock',
        leaseState: 'none',
      })
    ).toEqual({ mode: 'hidden' })
  })
})
