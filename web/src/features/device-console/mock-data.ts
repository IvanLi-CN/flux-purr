import type { DeviceStatus, TelemetryPoint, WifiConfig } from './types'

export const mockStatus: DeviceStatus = {
  mode: 'sampling',
  voltage: 28.01,
  current: 0.84,
  boardTempC: 34.6,
  pdRequestMv: 28000,
  pdContractMv: 28000,
  pdState: 'ready',
  usbRoute: 'mcu',
  fanEnabled: true,
  fanPwmPermille: 720,
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
  { ts: '20:01', voltage: 27.92, current: 0.73 },
  { ts: '20:02', voltage: 27.96, current: 0.77 },
  { ts: '20:03', voltage: 27.98, current: 0.8 },
  { ts: '20:04', voltage: 28.0, current: 0.82 },
  { ts: '20:05', voltage: 28.01, current: 0.84 },
]
