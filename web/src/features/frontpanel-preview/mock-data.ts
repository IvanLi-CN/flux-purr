import { frontPanelDefaultThresholdsC } from './design-tokens'
import type { FrontPanelScreen, MenuItemId } from './types'

const menuItems: ReadonlyArray<{ id: MenuItemId; label: string }> = [
  { id: 'preset-temp', label: 'Preset Temp' },
  { id: 'active-cooling', label: 'Active Cooling' },
  { id: 'wifi-info', label: 'WiFi Info' },
  { id: 'device-info', label: 'Device Info' },
]

export const frontPanelStoryStates = {
  home: {
    kind: 'home',
    title: 'Home',
    subtitle: 'Primary runtime screen',
    currentTempC: 365.4,
    targetTempC: 380.0,
    pwmPercent: 72,
    voltage: 20.08,
    protocol: 'PPS',
    fanState: 'on',
    pdState: 'ready',
    temperatureThresholdsC: frontPanelDefaultThresholdsC,
  } satisfies FrontPanelScreen,
  menu: {
    kind: 'menu',
    title: 'Preferences',
    subtitle: 'Horizontal icon selector',
    selectedItem: 'active-cooling',
    items: menuItems,
  } satisfies FrontPanelScreen,
  presetTemp: {
    kind: 'preset-temp',
    title: 'Preset Temp',
    subtitle: 'Preset temperature slots',
    selectedPresetIndex: 3,
    presetsC: [320, 340, null, 380, 400, null, 420, 450, null],
    temperatureThresholdsC: frontPanelDefaultThresholdsC,
  } satisfies FrontPanelScreen,
  presetTempDisabled: {
    kind: 'preset-temp',
    title: 'Preset Temp Disabled',
    subtitle: 'Disabled preset slot preview',
    selectedPresetIndex: 5,
    presetsC: [320, 340, null, 380, 400, null, 420, 450, null],
    temperatureThresholdsC: frontPanelDefaultThresholdsC,
  } satisfies FrontPanelScreen,
  activeCooling: {
    kind: 'active-cooling',
    title: 'Active Cooling',
    subtitle: 'Single-task cooling state page',
    enabled: true,
    mode: 'smart',
    fanState: 'auto',
  } satisfies FrontPanelScreen,
  wifiInfo: {
    kind: 'wifi-info',
    title: 'WiFi Info',
    subtitle: 'Compact connection summary',
    ssid: 'FluxLab',
    rssiDbm: -58,
    ipAddress: '192.168.4.1',
  } satisfies FrontPanelScreen,
  deviceInfo: {
    kind: 'device-info',
    title: 'Device Info',
    subtitle: 'Board and firmware identity',
    board: 'FP-S3',
    firmwareVersion: 'v0.2.0',
    serial: 'S3-001',
  } satisfies FrontPanelScreen,
} as const

export const frontPanelGalleryOrder: FrontPanelScreen[] = [
  frontPanelStoryStates.home,
  frontPanelStoryStates.menu,
  frontPanelStoryStates.presetTemp,
  frontPanelStoryStates.activeCooling,
  frontPanelStoryStates.wifiInfo,
  frontPanelStoryStates.deviceInfo,
]
