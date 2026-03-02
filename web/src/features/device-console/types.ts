export type DeviceMode = 'idle' | 'sampling' | 'fault'

export interface DeviceStatus {
  mode: DeviceMode
  voltage: number
  current: number
  boardTempC: number
  wifiRssi: number
  fwVersion: string
  lastSync: string
}

export interface WifiConfig {
  ssid: string
  passwordMasked: string
  autoReconnect: boolean
  telemetryIntervalMs: number
}

export interface TelemetryPoint {
  ts: string
  voltage: number
  current: number
}
