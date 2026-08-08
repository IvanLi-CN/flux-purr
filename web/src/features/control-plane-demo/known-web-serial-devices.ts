import type { DeviceTarget } from './types'

const STORAGE_KEY = 'flux-purr:known-web-serial-devices:v1'

interface StorageLike {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
}

export interface KnownWebSerialDevice {
  deviceId: string
  hostname: string
  firmwareVersion: string
  buildId: string
}

export function listKnownWebSerialDevices(
  storage: StorageLike | null = browserStorage()
): KnownWebSerialDevice[] {
  if (!storage) return []
  try {
    const value: unknown = JSON.parse(storage.getItem(STORAGE_KEY) ?? '[]')
    if (!Array.isArray(value)) return []
    return value.filter(isKnownWebSerialDevice)
  } catch {
    return []
  }
}

export function rememberKnownWebSerialDevice(
  device: KnownWebSerialDevice,
  storage: StorageLike | null = browserStorage()
) {
  if (!storage || !isKnownWebSerialDevice(device)) return
  const devices = listKnownWebSerialDevices(storage)
  const next = [device, ...devices.filter((candidate) => candidate.deviceId !== device.deviceId)]
  storage.setItem(STORAGE_KEY, JSON.stringify(next))
}

export function knownWebSerialDeviceToTarget(device: KnownWebSerialDevice): DeviceTarget {
  return {
    id: `web-serial-${device.deviceId}`,
    identityId: device.deviceId,
    alias: device.hostname || device.deviceId,
    location: 'Browser Web Serial',
    transport: 'serial',
    severity: 'offline',
    baseUrl: 'webserial://selected',
    firmware: device.firmwareVersion,
    buildId: device.buildId,
    uptime: 'N/A',
    boardTempC: 0,
    currentTempC: 0,
    targetTempC: 0,
    voltageMv: 0,
    currentMa: 0,
    pdRequestMv: 0,
    pdContractMv: 0,
    pdState: 'fault',
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
    fanState: 'OFF',
    wifiRssi: null,
    capabilities: ['identity', 'network', 'status', 'usb_jsonl'],
    networkState: 'idle',
    leaseState: 'none',
    transportIssue: '选择此通道后，浏览器将验证已授权串口的设备身份。',
  }
}

function isKnownWebSerialDevice(value: unknown): value is KnownWebSerialDevice {
  if (!value || typeof value !== 'object') return false
  const device = value as Record<string, unknown>
  return (
    typeof device.deviceId === 'string' &&
    device.deviceId.length > 0 &&
    typeof device.hostname === 'string' &&
    typeof device.firmwareVersion === 'string' &&
    typeof device.buildId === 'string'
  )
}

function browserStorage(): StorageLike | null {
  return typeof window === 'undefined' ? null : window.localStorage
}
