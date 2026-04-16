export type FrontPanelKeyId = 'center' | 'right' | 'down' | 'left' | 'up'
export type KeyGestureId = 'short' | 'double' | 'long'
export type CoolingMode = 'smart' | 'boost' | 'off'
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
  targetTempC: number
  heaterEnabled: boolean
  fanEnabled: boolean
  temperatureThresholdsC: readonly [number, number, number, number, number, number]
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
  temperatureThresholdsC: readonly [number, number, number, number, number, number]
}

export interface FrontPanelCoolingScreen extends FrontPanelBaseScreen {
  kind: 'active-cooling'
  enabled: boolean
  mode: CoolingMode
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
