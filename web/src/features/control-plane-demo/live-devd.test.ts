import { describe, expect, it, vi } from 'vitest'
import type { DevdDeviceRecord, DevdEvent, DevdLease } from './contracts'
import {
  createBootstrappingLiveDevdScenario,
  degradeDevicesForRefreshError,
  devdRecordToReconnectingTarget,
  preserveLastLiveDevdTarget,
  prioritizeLiveDevdDevices,
  readStoredDevdLeaseId,
  readStoredLiveDevdTarget,
  releaseDevdLeaseOnPageHide,
  resolveDevdLease,
  selectPreferredLiveDevdDeviceId,
  writeStoredDevdLeaseId,
  writeStoredLiveDevdTarget,
} from './live-devd'
import { liveControlPlaneScenario } from './live-scenario'
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

  it('preserves the stored lease during pagehide so a same-tab reload can heartbeat it', async () => {
    const storage = createMemoryStorage()
    const lease: DevdLease = { leaseId: 'lease-1', deviceId: 'native-2', ttlMs: 8000 }
    const fetcher = vi.fn().mockResolvedValue({ ok: true })

    releaseDevdLeaseOnPageHide({
      devdBaseUrl: 'http://127.0.0.1:56550',
      lease,
      deviceId: 'native-2',
      storage,
      fetcher: fetcher as unknown as typeof fetch,
    })

    expect(readStoredDevdLeaseId('http://127.0.0.1:56550', 'native-2', storage)).toBe('lease-1')
    expect(fetcher).toHaveBeenCalledWith('http://127.0.0.1:56550/api/v1/leases/lease-1', {
      method: 'DELETE',
      keepalive: true,
    })

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
  })

  it('keeps the last live devd target when refresh fails', () => {
    const devices: DeviceTarget[] = [
      makeDevice('native-1', 'devd', 'active'),
      makeDevice('fixture', 'mock', 'none'),
    ]

    const degraded = degradeDevicesForRefreshError(devices, new Error('Failed to fetch'))

    expect(degraded).toHaveLength(2)
    expect(selectPreferredLiveDevdDeviceId(degraded)).toBe('native-1')
    expect(degraded[0]).toMatchObject({
      id: 'native-1',
      transport: 'devd',
      severity: 'warning',
      networkState: 'error',
      transportIssue: 'Failed to fetch',
      leaseState: 'active',
    })
    expect(degraded[1]).toMatchObject({
      id: 'fixture',
      transport: 'mock',
      severity: 'nominal',
    })
  })

  it('creates a devd placeholder when refresh fails before any live target is known', () => {
    const degraded = degradeDevicesForRefreshError([], new Error('Failed to fetch'))

    expect(degraded).toHaveLength(1)
    expect(degraded[0]).toMatchObject({
      id: 'live-devd-unavailable',
      transport: 'devd',
      severity: 'warning',
      networkState: 'error',
      leaseState: 'none',
      transportIssue: 'Failed to fetch',
    })
  })

  it('keeps the last live devd target when the daemon returns no native targets', () => {
    const currentDevices: DeviceTarget[] = [
      makeDevice('native-1', 'devd', 'active'),
      makeDevice('fixture', 'mock', 'none'),
    ]
    const nextDevices = [makeDevice('fixture-2', 'mock', 'none')]

    const preserved = preserveLastLiveDevdTarget(nextDevices, currentDevices)

    expect(preserved).toHaveLength(2)
    expect(preserved[0]).toMatchObject({
      id: 'native-1',
      transport: 'devd',
      severity: 'warning',
      networkState: 'error',
      leaseState: 'active',
      transportIssue:
        'Authorized native serial target is temporarily unavailable; keeping the last live target until polling recovers.',
    })
    expect(preserved[1]).toMatchObject({
      id: 'fixture-2',
      transport: 'mock',
      severity: 'nominal',
    })
  })

  it('keeps a devd placeholder when the daemon returns no native targets before one is known', () => {
    const nextDevices = [makeDevice('fixture-2', 'mock', 'none')]

    const preserved = preserveLastLiveDevdTarget(nextDevices, [])

    expect(preserved).toHaveLength(2)
    expect(preserved[0]).toMatchObject({
      id: 'live-devd-unavailable',
      transport: 'devd',
      severity: 'warning',
      networkState: 'error',
      leaseState: 'none',
      transportIssue:
        'Authorized native serial target is temporarily unavailable; keeping the last live target until polling recovers.',
    })
    expect(preserved[1]).toMatchObject({
      id: 'fixture-2',
      transport: 'mock',
      severity: 'nominal',
    })
  })

  it('rehydrates the last live devd target as reconnecting during bootstrap', () => {
    const storage = createMemoryStorage()
    const device = makeDevice('native-1', 'devd', 'active')
    device.networkState = 'connected'
    device.transportIssue = null as unknown as string
    device.leaseId = 'lease-1'

    writeStoredLiveDevdTarget('http://127.0.0.1:56550', device, storage)

    const restored = readStoredLiveDevdTarget('http://127.0.0.1:56550', storage)

    expect(restored).toHaveLength(1)
    expect(restored[0]).toMatchObject({
      id: 'native-1',
      transport: 'devd',
      severity: 'warning',
      networkState: 'connecting',
      leaseState: 'expired',
      transportIssue: '正在重新接管本机 devd 租约，请稍候。',
    })
    expect(restored[0].leaseId).toBeUndefined()
  })

  it('renders a reconnecting devd bootstrap target before the first probe returns', () => {
    const scenario = createBootstrappingLiveDevdScenario(liveControlPlaneScenario)

    expect(scenario.selectedDeviceId).toBe('live-devd-bootstrapping')
    expect(scenario.devices).toHaveLength(1)
    expect(scenario.devices[0]).toMatchObject({
      id: 'live-devd-bootstrapping',
      transport: 'devd',
      severity: 'warning',
      networkState: 'connecting',
      leaseState: 'expired',
      transportIssue: '正在重新接管本机 devd 租约，请稍候。',
    })
    expect(scenario.events[0]).toMatchObject({
      source: 'devd',
      tone: 'warning',
      message: 'devd reachable; waiting for the first authorized native serial probe',
    })
  })

  it('keeps a busy native record in reconnecting state without changing its device id', () => {
    const reconnecting = devdRecordToReconnectingTarget(makeBusyRecord('native-1'))

    expect(reconnecting).toMatchObject({
      id: 'native-1',
      alias: 'Authorized USB target',
      location: '/dev/cu.usbmodem-native-1',
      transport: 'devd',
      severity: 'warning',
      networkState: 'connecting',
      leaseState: 'expired',
      transportIssue: '正在重新接管本机 devd 租约，请稍候。',
    })
    expect(reconnecting.leaseId).toBeUndefined()
  })

  it('ignores generic firmware log lines when surfacing the latest transport issue', async () => {
    const recordEvents: DevdEvent[] = [
      {
        id: 'event-log-line',
        timestamp: '100',
        deviceId: 'serial-1',
        kind: 'serial',
        message: 'native serial monitor line',
        payload: {
          code: 'firmware_log',
          line: 'I (247) boot: Disabling RNG early entropy source...',
        },
      },
      {
        id: 'event-rpc-failed',
        timestamp: '101',
        deviceId: 'serial-1',
        kind: 'serial',
        message: 'native serial RPC failed',
        payload: {
          stage: 'status',
          code: 'usb_response_timeout',
          retryable: true,
        },
      },
    ]

    const { devdEventToLogEntry, isMeaningfulDevdTransportIssueEvent } = await import(
      './transport-client'
    )

    const latest = [...recordEvents].reverse().find(isMeaningfulDevdTransportIssueEvent)

    expect(latest?.id).toBe('event-rpc-failed')
    expect(latest ? devdEventToLogEntry(latest).message : null).toBe(
      'native serial RPC failed: status / usb_response_timeout'
    )
  })

  it('keeps an existing device transport issue when later stream events are less specific', async () => {
    const currentDevices: DeviceTarget[] = [
      {
        ...makeDevice('native-1', 'devd', 'active'),
        networkState: 'error',
        transportIssue:
          'Authorized serial port /dev/cu.usbmodem21231401 is missing. Observed alternate Espressif serial ports: /dev/cu.usbmodem212101, /dev/cu.usbmodem212201.',
      },
    ]

    const streamEvents: DevdEvent[] = [
      {
        id: 'event-rpc-failed',
        timestamp: '101',
        deviceId: 'native-1',
        kind: 'serial',
        message: 'native serial RPC failed',
        payload: {
          stage: 'identity',
          code: 'serial_open_failed',
          retryable: true,
        },
      },
    ]

    const { prioritizeLiveDevdDevices } = await import('./live-devd')
    const { devdEventToLogEntry, isMeaningfulDevdTransportIssueEvent } = await import(
      './transport-client'
    )

    const latestByDevice: Record<string, string> = {}
    for (const event of [...streamEvents].reverse()) {
      if (!event.deviceId || latestByDevice[event.deviceId]) {
        continue
      }
      if (!isMeaningfulDevdTransportIssueEvent(event)) {
        continue
      }
      latestByDevice[event.deviceId] = devdEventToLogEntry(event).message
    }

    const merged = prioritizeLiveDevdDevices(currentDevices).map((device) =>
      !device.transportIssue && latestByDevice[device.id]
        ? {
            ...device,
            transportIssue: latestByDevice[device.id],
          }
        : device
    )

    expect(merged[0].transportIssue).toContain(
      'Authorized serial port /dev/cu.usbmodem21231401 is missing.'
    )
    expect(merged[0].transportIssue).not.toContain('native serial RPC failed')
  })

  it('keeps the device transport issue when the live probe throws a less specific error', () => {
    const liveRecord = {
      ...makeDevice('native-1', 'devd', 'active'),
      transportIssue:
        'Authorized serial port /dev/cu.usbmodem21231401 is missing. Observed alternate Espressif serial ports: /dev/cu.usbmodem212101, /dev/cu.usbmodem212201.',
    }

    const issueMessage = 'Failed to open serial port: No such file or directory'
    const issueDevice = {
      ...liveRecord,
      severity: 'warning' as const,
      leaseState: 'active' as const,
      leaseId: 'lease-1',
    }
    issueDevice.transportIssue ||= issueMessage

    expect(issueDevice.transportIssue).toContain(
      'Authorized serial port /dev/cu.usbmodem21231401 is missing.'
    )
    expect(issueDevice.transportIssue).not.toBe(issueMessage)
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
    heaterLockReason: null,
    activeCoolingEnabled: true,
    fanState: 'AUTO',
    wifiRssi: null,
    capabilities: [],
    leaseState,
  }
}

function makeBusyRecord(id: string): DevdDeviceRecord {
  return {
    id,
    displayName: 'Authorized USB target',
    portPath: `/dev/cu.usbmodem-${id}`,
    transport: 'native_serial',
    connection: 'busy',
    identity: {
      deviceId: id,
      firmwareVersion: '0.1.0',
      buildId: 'build-1',
      gitSha: 'abc',
      board: 'esp32-s3',
      apiVersion: '2026-05-29',
      protocolVersion: 'flux-purr.usb.v1',
      hostname: id,
      capabilities: ['identity', 'status', 'network', 'monitor', 'flash'],
    },
    network: {
      state: 'idle',
      ssid: null,
      ip: null,
      gateway: null,
      dns: [],
      wifiRssi: null,
      lastError: null,
    },
    status: {
      uptimeSeconds: 0,
      currentTempC: 0,
      targetTempC: 30,
      boardTempCenti: 0,
      voltageMv: 0,
      currentMa: 0,
      pdRequestMv: 0,
      pdContractMv: 0,
      pdState: 'fault',
      manualPpsEnabled: false,
      manualPpsMv: null,
      manualPpsMa: null,
      ppsCapabilityMinMv: null,
      ppsCapabilityMaxMv: null,
      ppsCapabilityMaxMa: null,
      manualPpsError: null,
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
      activeCoolingEnabled: false,
      fanDisplayState: 'OFF',
      selectedPresetSlot: 0,
      presetsC: [],
      heaterLockReason: null,
      frontpanelKey: null,
      mode: 'idle',
      fanEnabled: false,
      fanPwmPermille: 0,
      network: {
        state: 'idle',
        ssid: null,
        ip: null,
        gateway: null,
        dns: [],
        wifiRssi: null,
        lastError: null,
      },
    },
    events: [],
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
