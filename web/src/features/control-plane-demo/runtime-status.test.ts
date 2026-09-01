import { describe, expect, it } from 'vitest'
import {
  createPendingHeaterFeedback,
  deviceControlBlockReason,
  HEATER_CONFIRMATION_TIMEOUT_MS,
  heaterConfirmationNowMs,
  lanLeaseAcquisitionRequest,
  lanLeaseHeartbeatFailureDetail,
  resolvePendingHeaterConfirmation,
  shouldAcquireLanLease,
  shouldReacquireLanLeaseOnExplicitSelection,
  shouldReplacePassiveFeedbackWithHeaterLock,
} from './runtime-status'
import type { DeviceTarget } from './types'

function makeDevice(overrides: Partial<DeviceTarget> = {}): DeviceTarget {
  return {
    id: 'devd-target',
    alias: 'USB JTAG/serial debug unit',
    location: '/dev/cu.usbmodem21231401',
    transport: 'devd',
    severity: 'nominal',
    baseUrl: 'devd://devd-target',
    firmware: '0.1.0',
    buildId: 'local-build',
    uptime: '00:00:12',
    boardTempC: 23.4,
    currentTempC: 23.4,
    targetTempC: 235,
    voltageMv: 20_000,
    currentMa: 2_250,
    pdRequestMv: 20_000,
    pdContractMv: 20_000,
    pdState: 'ready',
    manualPpsEnabled: false,
    manualPpsMv: null,
    manualPpsMa: null,
    ppsCapabilityMinMv: 5_000,
    ppsCapabilityMaxMv: 16_000,
    ppsCapabilityMaxMa: 3_000,
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
    activeCoolingEnabled: true,
    fanState: 'AUTO',
    wifiRssi: null,
    capabilities: ['identity', 'status', 'network', 'monitor'],
    networkState: 'connected',
    leaseState: 'active',
    heaterLockReason: null,
    ...overrides,
  }
}

describe('pending heater confirmation', () => {
  it('uses the timer timestamp after the confirmation deadline fires', () => {
    expect(heaterConfirmationNowMs(1_000, 0)).toBe(1_000)
    expect(heaterConfirmationNowMs(1_000, 3_500)).toBe(3_500)
  })

  it('reports a neutral waiting state immediately after a live resume request', () => {
    expect(createPendingHeaterFeedback(true)).toEqual({
      title: 'Heater resume requested',
      detail: 'Waiting for firmware to keep the heater enabled.',
      tone: 'info',
    })
  })

  it('stays pending while the firmware has not yet reflected the requested heater state', () => {
    const resolution = resolvePendingHeaterConfirmation(
      {
        deviceId: 'devd-target',
        requestedEnabled: true,
        requestedAtMs: 1_000,
      },
      makeDevice({ heaterEnabled: false }),
      1_000 + HEATER_CONFIRMATION_TIMEOUT_MS - 1
    )

    expect(resolution).toEqual({ outcome: 'pending' })
  })

  it('confirms the request once the live status keeps the heater enabled', () => {
    const resolution = resolvePendingHeaterConfirmation(
      {
        deviceId: 'devd-target',
        requestedEnabled: true,
        requestedAtMs: 1_000,
      },
      makeDevice({ heaterEnabled: true, heaterOutputPercent: 18 }),
      1_200
    )

    expect(resolution).toMatchObject({
      outcome: 'confirmed',
      eventMessage: 'heater output resumed',
      eventTone: 'success',
      feedback: {
        title: 'Heater resumed',
        detail: 'Heater output follows the target temperature again.',
        tone: 'success',
      },
    })
  })

  it('surfaces the firmware safety lock when resume is rolled back', () => {
    const resolution = resolvePendingHeaterConfirmation(
      {
        deviceId: 'devd-target',
        requestedEnabled: true,
        requestedAtMs: 1_000,
      },
      makeDevice({
        heaterEnabled: false,
        heaterLockReason: 'cooling-disabled-overtemp',
      }),
      1_100
    )

    expect(resolution).toMatchObject({
      outcome: 'rejected',
      eventMessage: 'heater resume rolled back by firmware safety state',
      feedback: {
        title: 'Heater resume not confirmed',
        detail: '热板温度过高且主动散热已关闭，安全锁已关闭加热。',
        tone: 'warning',
      },
    })
  })

  it('marks the request as rejected after the confirmation window expires', () => {
    const resolution = resolvePendingHeaterConfirmation(
      {
        deviceId: 'devd-target',
        requestedEnabled: true,
        requestedAtMs: 1_000,
      },
      makeDevice({ heaterEnabled: false }),
      1_000 + HEATER_CONFIRMATION_TIMEOUT_MS
    )

    expect(resolution).toMatchObject({
      outcome: 'rejected',
      eventMessage: 'heater resume request was not sustained by firmware',
      feedback: {
        title: 'Heater resume not confirmed',
        detail: 'The latest firmware status returned to held before the heater could stay enabled.',
        tone: 'warning',
      },
    })
  })
})

describe('direct LAN lease guard', () => {
  it('preserves a protocol heartbeat failure instead of replacing it with a generic error', () => {
    expect(lanLeaseHeartbeatFailureDetail(' Another LAN client owns the lease. ')).toBe(
      'LAN lease 心跳失败：Another LAN client owns the lease.'
    )
    expect(lanLeaseHeartbeatFailureDetail('')).toBe('LAN lease 心跳失败，请重新选择设备。')
  })

  it('does not reacquire a heartbeat-expired lease or steal an explicit conflict', () => {
    expect(shouldAcquireLanLease({ leaseState: 'none' })).toBe(true)
    expect(shouldAcquireLanLease({ leaseState: 'active' })).toBe(true)
    expect(shouldAcquireLanLease({ leaseState: 'conflict' })).toBe(false)
    expect(shouldAcquireLanLease({ leaseState: 'expired' })).toBe(false)
  })

  it('only reopens an expired lease after the operator explicitly reselects direct LAN', () => {
    expect(
      shouldReacquireLanLeaseOnExplicitSelection({ transport: 'wifi', leaseState: 'expired' })
    ).toBe(true)
    expect(
      shouldReacquireLanLeaseOnExplicitSelection({ transport: 'devd', leaseState: 'expired' })
    ).toBe(false)
    expect(
      shouldReacquireLanLeaseOnExplicitSelection({ transport: 'wifi', leaseState: 'conflict' })
    ).toBe(false)
  })

  it('blocks direct LAN writes until the device confirms an active lease', () => {
    expect(
      deviceControlBlockReason(
        makeDevice({
          transport: 'wifi',
          baseUrl: 'http://192.168.1.18',
          leaseState: 'none',
        })
      )
    ).toBe('正在获取 LAN 控制租约，暂时无法下发控制。')

    expect(
      deviceControlBlockReason(
        makeDevice({
          transport: 'wifi',
          baseUrl: 'http://192.168.1.18',
          leaseState: 'active',
        })
      )
    ).toBeNull()
  })

  it('does not block USB runtime controls for a terminal WiFi provisioning outcome', () => {
    expect(
      deviceControlBlockReason(
        makeDevice({ transport: 'serial', networkState: 'timeout', leaseState: 'active' })
      )
    ).toBeNull()

    expect(
      deviceControlBlockReason(
        makeDevice({
          transport: 'devd',
          bridgeTransport: 'usb',
          networkState: 'error',
          leaseState: 'active',
        })
      )
    ).toBeNull()
  })

  it('continues to block network control transports for a terminal network failure', () => {
    expect(
      deviceControlBlockReason(
        makeDevice({ transport: 'wifi', networkState: 'timeout', leaseState: 'active' })
      )
    ).toBe('当前传输尚未恢复，暂时无法下发控制。')

    expect(
      deviceControlBlockReason(
        makeDevice({
          transport: 'devd',
          bridgeTransport: 'wifi',
          networkState: 'error',
          leaseState: 'active',
        })
      )
    ).toBe('当前传输尚未恢复，暂时无法下发控制。')
  })

  it('keeps LAN lease acquisition stable across unrelated status refreshes', () => {
    const target = makeDevice({
      id: 'lan-a0f262f20d6c',
      alias: 'flux-purr-a0f262f20d6c',
      transport: 'wifi',
      baseUrl: 'http://192.168.31.189',
      leaseState: 'none',
    })

    expect(lanLeaseAcquisitionRequest(target, false)).toEqual({
      deviceId: 'lan-a0f262f20d6c',
      baseUrl: 'http://192.168.31.189',
      alias: 'flux-purr-a0f262f20d6c',
    })
    const refreshedTarget = { ...target, currentTempC: 31.2, boardTempC: 32.1 }
    expect(lanLeaseAcquisitionRequest(refreshedTarget, false)).toEqual(
      lanLeaseAcquisitionRequest(target, false)
    )
    expect(lanLeaseAcquisitionRequest(target, true)).toBeNull()

    expect(lanLeaseAcquisitionRequest({ ...target, leaseState: 'expired' }, false)).toBeNull()
  })
})

describe('heater lock feedback priority', () => {
  it('does not replace an in-flight connection or result with a persistent heater lock reminder', () => {
    expect(shouldReplacePassiveFeedbackWithHeaterLock('正在连接 Web Serial')).toBe(false)
    expect(shouldReplacePassiveFeedbackWithHeaterLock('Web Serial unavailable')).toBe(false)
    expect(shouldReplacePassiveFeedbackWithHeaterLock('LAN 设备已连接')).toBe(false)
  })
})
