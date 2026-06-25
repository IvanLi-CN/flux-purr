import type { CalibrationMode } from './contracts'

export type CalibrationWorkspaceTab = 'heater_curve' | 'rtd_adc' | 'vin_adc'
export type ConsoleView = 'dashboard' | 'settings' | 'calibration' | 'update' | 'add-device'

export type CalibrationLeaveReason =
  | 'view-change'
  | 'device-change'
  | 'workspace-tab-change'
  | 'add-device-flow'

export interface CalibrationLeaveRequest {
  reason: CalibrationLeaveReason
  nextLabel: string
  nextView?: ConsoleView
  nextWorkspaceTab?: CalibrationWorkspaceTab
}

export function asCalibrationWorkbenchMode(mode: CalibrationMode): CalibrationWorkspaceTab | null {
  if (mode === 'vin_adc' || mode === 'rtd_adc' || mode === 'heater_curve') {
    return mode
  }
  return null
}

export function shouldBlockCalibrationViewChange(
  activeMode: CalibrationMode,
  currentView: ConsoleView,
  nextView: ConsoleView
) {
  return activeMode !== 'off' && currentView === 'calibration' && nextView !== 'calibration'
}

export function shouldBlockCalibrationWorkspaceTabChange(
  activeMode: CalibrationMode,
  currentTab: CalibrationWorkspaceTab,
  nextTab: CalibrationWorkspaceTab
) {
  const activeWorkbenchMode = asCalibrationWorkbenchMode(activeMode)
  if (activeWorkbenchMode == null) {
    return false
  }
  if (nextTab === currentTab) {
    return false
  }
  return activeWorkbenchMode === currentTab
}

export function shouldBlockCalibrationDeviceChange(
  activeMode: CalibrationMode,
  currentView: ConsoleView
) {
  return activeMode !== 'off' && currentView === 'calibration'
}
