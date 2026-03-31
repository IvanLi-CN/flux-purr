export type DeviceMode = 'idle' | 'sampling' | 'fault'
export type PdState = 'negotiating' | 'ready' | 'fallback_5v' | 'fault'
export type FrontPanelKey = 'center' | 'right' | 'down' | 'left' | 'up'

export interface DeviceStatus {
  mode: DeviceMode
  voltage: number
  current: number
  boardTempC: number
  pdRequestMv: number
  pdContractMv: number
  pdState: PdState
  fanEnabled: boolean
  fanPwmPermille: number
  frontpanelKey: FrontPanelKey | null
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
