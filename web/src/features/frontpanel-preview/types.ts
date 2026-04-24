export type FrontPanelKeyId = 'center' | 'right' | 'down' | 'left' | 'up'
export type KeyGestureId = 'short' | 'double' | 'long'
export type FanDisplayState = 'off' | 'auto' | 'run'
export type HeaterLockReason = 'cooling-disabled-overtemp' | 'hard-overtemp'
export type MenuItemId = 'preset-temp' | 'active-cooling' | 'wifi-info' | 'device-info'

interface FrontPanelBaseScreen {
  title: string
  subtitle?: string
}

export interface FrontPanelKeyTestScreen extends FrontPanelBaseScreen {
  kind: 'key-test'
  activeKey: FrontPanelKeyId | null
  activeGesture: KeyGestureId | null
  rawKeyLabel: string
  logicalKeyLabel: string
  gestureLabel: string
  rawMaskLabel: string
}

export interface FrontPanelDashboardScreen extends FrontPanelBaseScreen {
  kind: 'dashboard'
  currentTempC: number
  currentTempDeciC: number
  targetTempC: number
  heaterEnabled: boolean
  heaterOutputPercent: number
  fanRuntimeEnabled: boolean
  fanDisplayState: FanDisplayState
  pdContractMv: number
  heaterLockReason: HeaterLockReason | null
  dashboardWarningVisible: boolean
  temperatureThresholdsC: readonly number[]
}

export interface FrontPanelMenuScreen extends FrontPanelBaseScreen {
  kind: 'menu'
  selectedItem: MenuItemId
  items: ReadonlyArray<{
    id: MenuItemId
    label: string
  }>
}

export interface FrontPanelPresetTempScreen extends FrontPanelBaseScreen {
  kind: 'preset-temp'
  selectedPresetIndex: number
  presetsC: ReadonlyArray<number | null>
  temperatureThresholdsC: readonly number[]
}

export interface FrontPanelCoolingScreen extends FrontPanelBaseScreen {
  kind: 'active-cooling'
  enabled: boolean
  pdContractMv: number
  autoStopTempC: number
  autoStartTempC: number
  autoFullTempC: number
  pulseStartTempC: number
  lockTempC: number
  fullTempC: number
}

export interface FrontPanelWifiInfoScreen extends FrontPanelBaseScreen {
  kind: 'wifi-info'
  ssid: string
  rssiDbm: number
  ipAddress: string
}

export interface FrontPanelDeviceInfoScreen extends FrontPanelBaseScreen {
  kind: 'device-info'
  board: string
  firmwareVersion: string
  serial: string
}

export type FrontPanelScreen =
  | FrontPanelKeyTestScreen
  | FrontPanelDashboardScreen
  | FrontPanelMenuScreen
  | FrontPanelPresetTempScreen
  | FrontPanelCoolingScreen
  | FrontPanelWifiInfoScreen
  | FrontPanelDeviceInfoScreen
