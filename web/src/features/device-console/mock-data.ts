import type { DeviceStatus, TelemetryPoint, WifiConfig } from './types'

export const mockStatus: DeviceStatus = {
  mode: 'sampling',
  voltage: 12.08,
  current: 0.83,
  boardTempC: 34.6,
  wifiRssi: -58,
  fwVersion: 'fw/v0.1.0-dev',
  lastSync: '2026-03-02T18:05:00+08:00',
}

export const mockWifiConfig: WifiConfig = {
  ssid: 'FluxPurr-Lab',
  passwordMasked: '••••••••',
  autoReconnect: true,
  telemetryIntervalMs: 500,
}

export const mockTelemetrySeries: TelemetryPoint[] = [
  { ts: '18:01', voltage: 12.02, current: 0.71 },
  { ts: '18:02', voltage: 12.03, current: 0.74 },
  { ts: '18:03', voltage: 12.06, current: 0.78 },
  { ts: '18:04', voltage: 12.07, current: 0.81 },
  { ts: '18:05', voltage: 12.08, current: 0.83 },
]
