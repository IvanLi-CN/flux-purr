import type { DeviceStatus, TelemetryPoint, WifiConfig } from './types'

export const mockStatus: DeviceStatus = {
  mode: 'sampling',
  voltage: 20.01,
  current: 0.84,
  boardTempC: 34.6,
  pdRequestMv: 20000,
  pdContractMv: 20000,
  pdState: 'ready',
  fanEnabled: true,
  fanPwmPermille: 500,
  frontpanelKey: 'center',
  wifiRssi: -58,
  fwVersion: 'fw/v0.2.0-dev',
  lastSync: '2026-03-03T20:05:00+08:00',
}

export const mockWifiConfig: WifiConfig = {
  ssid: 'FluxPurr-Lab',
  passwordMasked: '••••••••',
  autoReconnect: true,
  telemetryIntervalMs: 500,
}

export const mockTelemetrySeries: TelemetryPoint[] = [
  { ts: '20:01', voltage: 19.92, current: 0.73 },
  { ts: '20:02', voltage: 19.96, current: 0.77 },
  { ts: '20:03', voltage: 19.98, current: 0.8 },
  { ts: '20:04', voltage: 20.0, current: 0.82 },
  { ts: '20:05', voltage: 20.01, current: 0.84 },
]
