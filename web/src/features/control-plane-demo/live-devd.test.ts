import { describe, expect, it, vi } from 'vitest'
import type { DevdLease } from './contracts'
import {
  prioritizeLiveDevdDevices,
  readStoredDevdLeaseId,
  resolveDevdLease,
  selectPreferredLiveDevdDeviceId,
  writeStoredDevdLeaseId,
} from './live-devd'
import type { ControlPlaneHttpClient } from './transport-client'
import type { DeviceTarget } from './types'

describe('live devd selection', () => {
  it('prefers active devd targets before fixtures', () => {
    const devices: DeviceTarget[] = [
      makeDevice('fixture', 'mock', 'none'),
      makeDevice('native-1', 'devd', 'conflict'),
      makeDevice('native-2', 'devd', 'active'),
    ]

    const sorted = prioritizeLiveDevdDevices(devices)

    expect(sorted[0].id).toBe('native-2')
    expect(selectPreferredLiveDevdDeviceId(sorted)).toBe('native-2')
  })

  it('reuses a stored lease before creating a new one', async () => {
    const storage = createMemoryStorage()
    const lease: DevdLease = { leaseId: 'lease-1', deviceId: 'native-2', ttlMs: 8000 }
    writeStoredDevdLeaseId('http://127.0.0.1:56550', 'native-2', 'lease-1', storage)

    const client = {
      heartbeatDevdLease: vi.fn().mockResolvedValue(lease),
      createDevdLease: vi.fn(),
      releaseDevdLease: vi.fn(),
    } as unknown as ControlPlaneHttpClient

    const resolved = await resolveDevdLease({
      client,
      devdBaseUrl: 'http://127.0.0.1:56550',
      deviceId: 'native-2',
      currentLease: null,
      currentLeaseDeviceId: null,
      storage,
      cancelled: () => false,
    })

    expect(resolved).toEqual(lease)
    expect(client.heartbeatDevdLease).toHaveBeenCalledWith('http://127.0.0.1:56550', 'lease-1')
    expect(client.createDevdLease).not.toHaveBeenCalled()
    expect(readStoredDevdLeaseId('http://127.0.0.1:56550', 'native-2', storage)).toBe('lease-1')
  })
})

function makeDevice(
  id: string,
  transport: DeviceTarget['transport'],
  leaseState: DeviceTarget['leaseState']
): DeviceTarget {
  return {
    id,
    alias: id,
    location: 'test',
    transport,
    severity: 'nominal',
    baseUrl: 'test://device',
    firmware: '0.1.0',
    buildId: 'build',
    uptime: '00:00:00',
    boardTempC: 25,
    currentTempC: 25,
    targetTempC: 25,
    rtdRawAdcMv: 1100,
    vinRawAdcMv: 1670,
    voltageMv: 20000,
    currentMa: 1000,
    pdRequestMv: 20000,
    pdContractMv: 20000,
    pdState: 'ready',
    calibration: {
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
    },
    heaterEnabled: false,
    heaterOutputPercent: 0,
    activeCoolingEnabled: true,
    fanState: 'AUTO',
    wifiRssi: null,
    capabilities: [],
    leaseState,
  }
}

function createMemoryStorage() {
  const store = new Map<string, string>()
  return {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, value)
    },
    removeItem: (key: string) => {
      store.delete(key)
    },
  }
}
