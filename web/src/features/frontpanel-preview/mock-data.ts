import {
  buildRuntimeScreenSnapshot,
  type FrontPanelRuntimeInteraction,
  frontPanelRuntimeToScreen,
} from './runtime'
import type { FrontPanelScreen } from './types'

function repeatInteraction(
  count: number,
  key: FrontPanelRuntimeInteraction['key'],
  gesture: FrontPanelRuntimeInteraction['gesture']
): FrontPanelRuntimeInteraction[] {
  return Array.from({ length: count }, () => ({ key, gesture }))
}

function screenFor(
  mode: 'key-test' | 'app',
  interactions: ReadonlyArray<FrontPanelRuntimeInteraction> = []
): FrontPanelScreen {
  return frontPanelRuntimeToScreen(buildRuntimeScreenSnapshot(mode, interactions))
}

export const frontPanelStoryStates = {
  keyTestIdle: screenFor('key-test'),
  keyTestShort: screenFor('key-test', [{ key: 'up', gesture: 'short' }]),
  keyTestDouble: screenFor('key-test', [{ key: 'center', gesture: 'double' }]),
  keyTestLong: screenFor('key-test', [{ key: 'left', gesture: 'long' }]),
  dashboard: screenFor('app'),
  dashboardManual: screenFor('app', [
    ...repeatInteraction(9, 'up', 'short'),
    { key: 'center', gesture: 'short' },
  ]),
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
  frontPanelStoryStates.menu,
  frontPanelStoryStates.presetTemp,
  frontPanelStoryStates.activeCooling,
  frontPanelStoryStates.wifiInfo,
  frontPanelStoryStates.deviceInfo,
]
