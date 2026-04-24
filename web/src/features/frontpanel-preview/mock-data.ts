import {
  buildRuntimeScreenSnapshot,
  createFrontPanelRuntimeState,
  type FrontPanelRuntimeInteraction,
  type FrontPanelRuntimeState,
  frontPanelRuntimeToScreen,
} from './runtime'
import type { FrontPanelScreen } from './types'

function screenFor(
  mode: 'key-test' | 'app',
  interactions: ReadonlyArray<FrontPanelRuntimeInteraction> = []
): FrontPanelScreen {
  return frontPanelRuntimeToScreen(buildRuntimeScreenSnapshot(mode, interactions))
}

function screenFromState(overrides: Partial<FrontPanelRuntimeState>): FrontPanelScreen {
  return frontPanelRuntimeToScreen({
    ...createFrontPanelRuntimeState('app'),
    ...overrides,
  })
}

export const frontPanelStoryStates = {
  keyTestIdle: screenFor('key-test'),
  keyTestShort: screenFor('key-test', [{ key: 'up', gesture: 'short' }]),
  keyTestDouble: screenFor('key-test', [{ key: 'center', gesture: 'double' }]),
  keyTestLong: screenFor('key-test', [{ key: 'left', gesture: 'long' }]),
  dashboard: screenFor('app'),
  dashboardManual: screenFromState({
    currentTempC: 365,
    currentTempDeciC: 3654,
    targetTempC: 380,
    heaterEnabled: true,
    heaterOutputPercent: 64,
    fanRuntimeEnabled: true,
    fanDisplayState: 'run',
  }),
  dashboardFanOff: screenFromState({
    currentTempC: 96,
    currentTempDeciC: 962,
    heaterEnabled: false,
    heaterOutputPercent: 0,
    activeCoolingEnabled: false,
    fanRuntimeEnabled: false,
    fanDisplayState: 'off',
  }),
  dashboardFanAuto: screenFromState({
    currentTempC: 34,
    currentTempDeciC: 341,
    heaterEnabled: false,
    heaterOutputPercent: 0,
    fanRuntimeEnabled: false,
    fanDisplayState: 'auto',
  }),
  dashboardFanRun: screenFromState({
    currentTempC: 58,
    currentTempDeciC: 583,
    heaterEnabled: false,
    heaterOutputPercent: 0,
    fanRuntimeEnabled: true,
    fanDisplayState: 'run',
  }),
  dashboardOvertempA: screenFromState({
    currentTempC: 351,
    currentTempDeciC: 3512,
    heaterEnabled: false,
    heaterOutputPercent: 0,
    activeCoolingEnabled: false,
    fanRuntimeEnabled: true,
    fanDisplayState: 'off',
    heaterLockReason: 'cooling-disabled-overtemp',
    dashboardWarningVisible: true,
  }),
  dashboardOvertempB: screenFromState({
    currentTempC: 351,
    currentTempDeciC: 3512,
    heaterEnabled: false,
    heaterOutputPercent: 0,
    activeCoolingEnabled: false,
    fanRuntimeEnabled: true,
    fanDisplayState: 'off',
    heaterLockReason: 'cooling-disabled-overtemp',
    dashboardWarningVisible: false,
  }),
  menu: screenFor('app', [{ key: 'center', gesture: 'long' }]),
  presetTemp: screenFor('app', [
    { key: 'center', gesture: 'long' },
    { key: 'left', gesture: 'short' },
    { key: 'center', gesture: 'short' },
  ]),
  activeCooling: screenFor('app', [
    { key: 'center', gesture: 'long' },
    { key: 'center', gesture: 'short' },
  ]),
  wifiInfo: screenFor('app', [
    { key: 'center', gesture: 'long' },
    { key: 'right', gesture: 'short' },
    { key: 'center', gesture: 'short' },
  ]),
  deviceInfo: screenFor('app', [
    { key: 'center', gesture: 'long' },
    { key: 'right', gesture: 'short' },
    { key: 'right', gesture: 'short' },
    { key: 'center', gesture: 'short' },
  ]),
} as const

export const frontPanelGalleryOrder: FrontPanelScreen[] = [
  frontPanelStoryStates.keyTestIdle,
  frontPanelStoryStates.dashboard,
  frontPanelStoryStates.dashboardFanOff,
  frontPanelStoryStates.dashboardFanAuto,
  frontPanelStoryStates.dashboardFanRun,
  frontPanelStoryStates.dashboardOvertempA,
  frontPanelStoryStates.dashboardOvertempB,
  frontPanelStoryStates.menu,
  frontPanelStoryStates.presetTemp,
  frontPanelStoryStates.activeCooling,
  frontPanelStoryStates.wifiInfo,
  frontPanelStoryStates.deviceInfo,
]
