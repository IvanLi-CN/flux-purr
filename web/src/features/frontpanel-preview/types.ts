export type PdState = 'ready' | 'negotiating' | 'fault'
export type FanState = 'on' | 'off' | 'auto'
export type CoolingMode = 'smart' | 'boost' | 'off'
export type MenuItemId = 'preset-temp' | 'active-cooling' | 'wifi-info' | 'device-info'
export type PowerProtocol = 'PD' | 'PPS' | 'VIN'

interface FrontPanelBaseScreen {
  title: string
  subtitle?: string
}

export interface FrontPanelHomeScreen extends FrontPanelBaseScreen {
  kind: 'home'
  currentTempC: number
  targetTempC: number
  pwmPercent: number
  voltage: number
  protocol: PowerProtocol
  fanState: FanState
  pdState: PdState
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
  fanState: FanState
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
  | FrontPanelHomeScreen
  | FrontPanelMenuScreen
  | FrontPanelPresetTempScreen
  | FrontPanelCoolingScreen
  | FrontPanelWifiInfoScreen
  | FrontPanelDeviceInfoScreen
