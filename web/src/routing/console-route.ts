import type {
  CalibrationWorkspaceTab,
  ConsoleView,
} from '@/features/control-plane-demo/calibration-leave-guard'

export type DeviceConsoleView = Exclude<ConsoleView, 'add-device'>

export type ConsoleRouteState =
  | { kind: 'add-device' }
  | {
      kind: 'device'
      deviceId: string
      view: DeviceConsoleView
      calibrationTab?: CalibrationWorkspaceTab
    }

const calibrationPathToTab = {
  'heater-curve': 'heater_curve',
  'rtd-adc': 'rtd_adc',
  'vin-adc': 'vin_adc',
} as const

const calibrationTabToPath = {
  heater_curve: 'heater-curve',
  rtd_adc: 'rtd-adc',
  vin_adc: 'vin-adc',
} as const

export function parseConsoleRoute(pathname: string): ConsoleRouteState | null {
  if (pathname === '/devices/new') return { kind: 'add-device' }
  const segments = pathname.split('/').filter(Boolean)
  if (segments[0] !== 'devices' || segments.length < 3) return null
  let deviceId: string
  try {
    deviceId = decodeURIComponent(segments[1] ?? '')
  } catch {
    return null
  }
  if (!deviceId) return null
  const view = segments[2]
  if ((view === 'overview' || view === 'settings' || view === 'update') && segments.length === 3) {
    return { kind: 'device', deviceId, view: view === 'overview' ? 'dashboard' : view }
  }
  if (view === 'calibration' && segments.length === 4) {
    const tab = calibrationPathToTab[segments[3] as keyof typeof calibrationPathToTab]
    return tab ? { kind: 'device', deviceId, view: 'calibration', calibrationTab: tab } : null
  }
  return null
}

export function consoleRoutePath(state: ConsoleRouteState) {
  if (state.kind === 'add-device') return '/devices/new'
  const base = `/devices/${encodeURIComponent(state.deviceId)}`
  if (state.view === 'dashboard') return `${base}/overview`
  if (state.view === 'calibration') {
    return `${base}/calibration/${calibrationTabToPath[state.calibrationTab ?? 'heater_curve']}`
  }
  return `${base}/${state.view}`
}

export function routeLabel(state: ConsoleRouteState | null) {
  if (!state || state.kind === 'add-device') return '添加设备'
  if (state.view === 'dashboard') return '总览'
  if (state.view === 'settings') return '设置'
  if (state.view === 'update') return '更新'
  const labels: Record<CalibrationWorkspaceTab, string> = {
    heater_curve: '加热曲线标定',
    rtd_adc: '温度标定',
    vin_adc: '电压读数标定',
  }
  return labels[state.calibrationTab ?? 'heater_curve']
}
