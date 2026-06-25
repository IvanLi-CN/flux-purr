import type { HeaterLockReason } from './contracts'
import type { DeviceTarget } from './types'

const BLOCKED_NETWORK_STATES = new Set(['error', 'timeout'])

export const HEATER_CONFIRMATION_TIMEOUT_MS = 2_500

export interface RuntimeFeedback {
  title: string
  detail: string
  tone: 'info' | 'success' | 'warning'
}

export interface PendingHeaterConfirmation {
  deviceId: string
  requestedEnabled: boolean
  requestedAtMs: number
}

export type PendingHeaterResolution =
  | { outcome: 'pending' }
  | {
      outcome: 'confirmed'
      eventMessage: string
      eventTone: 'success' | 'warning'
      feedback: RuntimeFeedback
    }
  | {
      outcome: 'rejected'
      eventMessage: string
      eventTone: 'warning'
      feedback: RuntimeFeedback
    }

export function heaterLockReasonText(reason: HeaterLockReason) {
  switch (reason) {
    case 'cooling-disabled-overtemp':
      return '热板温度过高且主动散热已关闭，安全锁已关闭加热。'
    case 'hard-overtemp':
      return '热板进入硬过温保护，安全锁已关闭加热。'
    default:
      return '加热已被固件安全锁关闭。'
  }
}

export function deviceControlBlockReason(
  device: Pick<DeviceTarget, 'severity' | 'leaseState' | 'transportIssue' | 'networkState'>
) {
  if (device.severity === 'offline') {
    return '目标设备当前离线。'
  }

  if (device.leaseState === 'conflict') {
    return device.transportIssue ?? '另一个页面或客户端占用了当前 USB 租约。'
  }

  if (device.leaseState === 'expired') {
    return device.transportIssue ?? '当前设备租约已过期，请等待页面重新接管。'
  }

  const networkState = device.networkState
  if (networkState && BLOCKED_NETWORK_STATES.has(networkState)) {
    return device.transportIssue ?? '当前传输尚未恢复，暂时无法下发控制。'
  }

  return null
}

export function runtimeHeaterState(
  device: Pick<DeviceTarget, 'severity' | 'heaterLockReason' | 'pdState' | 'heaterOutputPercent'>
) {
  if (device.severity === 'offline') {
    return 'offline'
  }
  if (device.heaterLockReason) {
    return 'locked'
  }
  if (device.pdState !== 'ready') {
    return device.pdState
  }
  return device.heaterOutputPercent > 0 ? 'holding' : 'held'
}

export function createPendingHeaterFeedback(requestedEnabled: boolean): RuntimeFeedback {
  return requestedEnabled
    ? {
        title: 'Heater resume requested',
        detail: 'Waiting for firmware to keep the heater enabled.',
        tone: 'info',
      }
    : {
        title: 'Heater hold requested',
        detail: 'Waiting for firmware to hold heater output at 0%.',
        tone: 'info',
      }
}

export function resolvePendingHeaterConfirmation(
  pending: PendingHeaterConfirmation,
  device: Pick<
    DeviceTarget,
    | 'id'
    | 'heaterEnabled'
    | 'heaterLockReason'
    | 'severity'
    | 'leaseState'
    | 'transportIssue'
    | 'networkState'
  >,
  nowMs = Date.now()
): PendingHeaterResolution {
  if (device.id !== pending.deviceId) {
    return { outcome: 'pending' }
  }

  const requestedEnabled = pending.requestedEnabled
  if (device.heaterEnabled === requestedEnabled) {
    return {
      outcome: 'confirmed',
      eventMessage: requestedEnabled ? 'heater output resumed' : 'heater output held at 0%',
      eventTone: requestedEnabled ? 'success' : 'warning',
      feedback: requestedEnabled
        ? {
            title: 'Heater resumed',
            detail: 'Heater output follows the target temperature again.',
            tone: 'success',
          }
        : {
            title: 'Heater held',
            detail: 'Heater output is disabled until resumed again.',
            tone: 'warning',
          },
    }
  }

  const blockedReason = deviceControlBlockReason(device)
  if (requestedEnabled && device.heaterLockReason) {
    return {
      outcome: 'rejected',
      eventMessage: 'heater resume rolled back by firmware safety state',
      eventTone: 'warning',
      feedback: {
        title: 'Heater resume not confirmed',
        detail: heaterLockReasonText(device.heaterLockReason),
        tone: 'warning',
      },
    }
  }

  if (blockedReason) {
    return {
      outcome: 'rejected',
      eventMessage: requestedEnabled
        ? 'heater resume rolled back by transport state'
        : 'heater hold rolled back by transport state',
      eventTone: 'warning',
      feedback: {
        title: 'Runtime update blocked',
        detail: blockedReason,
        tone: 'warning',
      },
    }
  }

  if (nowMs - pending.requestedAtMs < HEATER_CONFIRMATION_TIMEOUT_MS) {
    return { outcome: 'pending' }
  }

  return {
    outcome: 'rejected',
    eventMessage: requestedEnabled
      ? 'heater resume request was not sustained by firmware'
      : 'heater hold request was not sustained by firmware',
    eventTone: 'warning',
    feedback: {
      title: requestedEnabled ? 'Heater resume not confirmed' : 'Heater hold not confirmed',
      detail: requestedEnabled
        ? 'The latest firmware status returned to held before the heater could stay enabled.'
        : 'The latest firmware status re-enabled the heater before the hold request could persist.',
      tone: 'warning',
    },
  }
}
