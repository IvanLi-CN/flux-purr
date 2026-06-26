import { describe, expect, it } from 'vitest'
import {
  createPendingHeaterFeedback,
  HEATER_CONFIRMATION_TIMEOUT_MS,
  resolvePendingHeaterConfirmation,
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
