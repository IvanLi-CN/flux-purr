import { frontPanelDefaultThresholdsC } from './design-tokens'
import type {
  CoolingMode,
  FrontPanelDashboardScreen,
  FrontPanelKeyId,
  FrontPanelKeyTestScreen,
  FrontPanelScreen,
  KeyGestureId,
  MenuItemId,
} from './types'

export type FrontPanelRoute =
  | 'key-test'
  | 'dashboard'
  | 'menu'
  | 'preset-temp'
  | 'active-cooling'
  | 'wifi-info'
  | 'device-info'

export type FrontPanelRuntimeMode = 'key-test' | 'app'

export interface FrontPanelRuntimeInteraction {
  key: FrontPanelKeyId
  gesture: KeyGestureId
}

export interface FrontPanelRuntimeState {
  mode: FrontPanelRuntimeMode
  route: FrontPanelRoute
  targetTempC: number
  heaterEnabled: boolean
  fanEnabled: boolean
  selectedMenuItem: MenuItemId
  selectedPresetIndex: number
  presetsC: ReadonlyArray<number | null>
  activeCoolingEnabled: boolean
  activeCoolingMode: CoolingMode
  keyTest: {
    activeKey: FrontPanelKeyId | null
    activeGesture: KeyGestureId | null
    rawKeyLabel: string
    logicalKeyLabel: string
    gestureLabel: string
    rawMaskLabel: string
  }
}

const menuItems: ReadonlyArray<{ id: MenuItemId; label: string }> = [
  { id: 'preset-temp', label: 'Preset Temp' },
  { id: 'active-cooling', label: 'Active Cooling' },
  { id: 'wifi-info', label: 'WiFi Info' },
  { id: 'device-info', label: 'Device Info' },
]

const keyMaskMap: Record<FrontPanelKeyId, string> = {
  center: 'MASK 01',
  right: 'MASK 02',
  down: 'MASK 04',
  left: 'MASK 08',
  up: 'MASK 10',
}

const rawLabelMap: Record<FrontPanelKeyId, string> = {
  center: 'RAW CENTER',
  right: 'RAW RIGHT',
  down: 'RAW DOWN',
  left: 'RAW LEFT',
  up: 'RAW UP',
}

const logicalLabelMap: Record<FrontPanelKeyId, string> = {
  center: 'CENTER',
  right: 'RIGHT',
  down: 'DOWN',
  left: 'LEFT',
  up: 'UP',
}

export function createFrontPanelRuntimeState(
  mode: FrontPanelRuntimeMode = 'app'
): FrontPanelRuntimeState {
  return {
    mode,
    route: mode === 'key-test' ? 'key-test' : 'dashboard',
    targetTempC: 380,
    heaterEnabled: false,
    fanEnabled: false,
    selectedMenuItem: 'active-cooling',
    selectedPresetIndex: 3,
    presetsC: [320, 340, null, 380, 400, null, 420, 450, null],
    activeCoolingEnabled: true,
    activeCoolingMode: 'smart',
    keyTest: {
      activeKey: null,
      activeGesture: null,
      rawKeyLabel: 'RAW ---',
      logicalKeyLabel: 'LOG ---',
      gestureLabel: 'IDLE',
      rawMaskLabel: 'MASK 00',
    },
  }
}

function sortedActivePresetEntries(state: FrontPanelRuntimeState) {
  return state.presetsC
    .map((tempC, index) => ({ index, tempC }))
    .filter((entry): entry is { index: number; tempC: number } => entry.tempC != null)
    .sort((left, right) => left.tempC - right.tempC || left.index - right.index)
}

function findNeighborPreset(state: FrontPanelRuntimeState, ascending: boolean) {
  const entries = sortedActivePresetEntries(state)
  if (ascending) {
    return entries.find((entry) => entry.tempC > state.targetTempC) ?? null
  }
  return [...entries].reverse().find((entry) => entry.tempC < state.targetTempC) ?? null
}

function nextSortedPreset(state: FrontPanelRuntimeState) {
  if (!state.presetsC.length) return null
  const nextIndex = (state.selectedPresetIndex + 1) % state.presetsC.length
  return { index: nextIndex, tempC: state.presetsC[nextIndex] }
}

function updateKeyTest(
  state: FrontPanelRuntimeState,
  interaction: FrontPanelRuntimeInteraction
): FrontPanelRuntimeState {
  return {
    ...state,
    keyTest: {
      activeKey: interaction.key,
      activeGesture: interaction.gesture,
      rawKeyLabel: rawLabelMap[interaction.key],
      logicalKeyLabel: logicalLabelMap[interaction.key],
      gestureLabel: interaction.gesture.toUpperCase(),
      rawMaskLabel: keyMaskMap[interaction.key],
    },
  }
}

export function applyFrontPanelInteraction(
  current: FrontPanelRuntimeState,
  interaction: FrontPanelRuntimeInteraction
): FrontPanelRuntimeState {
  const state = updateKeyTest(current, interaction)
  if (state.mode === 'key-test') return state

  if (state.route === 'dashboard') {
    if (interaction.key === 'up' && interaction.gesture === 'short') {
      return { ...state, targetTempC: state.targetTempC + 1 }
    }
    if (interaction.key === 'down' && interaction.gesture === 'short') {
      return { ...state, targetTempC: state.targetTempC - 1 }
    }
    if (interaction.key === 'left' && interaction.gesture === 'short') {
      const neighbor = findNeighborPreset(state, false)
      return neighbor
        ? { ...state, targetTempC: neighbor.tempC, selectedPresetIndex: neighbor.index }
        : state
    }
    if (interaction.key === 'right' && interaction.gesture === 'short') {
      const neighbor = findNeighborPreset(state, true)
      return neighbor
        ? { ...state, targetTempC: neighbor.tempC, selectedPresetIndex: neighbor.index }
        : state
    }
    if (interaction.key === 'center' && interaction.gesture === 'short') {
      return { ...state, heaterEnabled: !state.heaterEnabled }
    }
    if (interaction.key === 'center' && interaction.gesture === 'double') {
      return { ...state, fanEnabled: !state.fanEnabled }
    }
    if (interaction.key === 'center' && interaction.gesture === 'long') {
      return { ...state, route: 'menu' }
    }
    return state
  }

  if (state.route === 'menu') {
    if (interaction.key === 'left' && interaction.gesture === 'short') {
      const index = menuItems.findIndex((item) => item.id === state.selectedMenuItem)
      return {
        ...state,
        selectedMenuItem: menuItems[(index + menuItems.length - 1) % menuItems.length].id,
      }
    }
    if (interaction.key === 'right' && interaction.gesture === 'short') {
      const index = menuItems.findIndex((item) => item.id === state.selectedMenuItem)
      return { ...state, selectedMenuItem: menuItems[(index + 1) % menuItems.length].id }
    }
    if (interaction.key === 'center' && interaction.gesture === 'short') {
      return {
        ...state,
        route: state.selectedMenuItem,
      }
    }
    if (interaction.key === 'center' && interaction.gesture === 'long') {
      return { ...state, route: 'dashboard' }
    }
    return state
  }

  if (state.route === 'preset-temp') {
    if (interaction.key === 'right' && interaction.gesture === 'short') {
      const entry = nextSortedPreset(state)
      return entry ? { ...state, selectedPresetIndex: entry.index } : state
    }
    if (interaction.key === 'up' && interaction.gesture === 'short') {
      const nextPresets = [...state.presetsC]
      const currentValue = nextPresets[state.selectedPresetIndex]
      const nextValue = currentValue == null ? 0 : currentValue + 1
      nextPresets[state.selectedPresetIndex] = nextValue
      return { ...state, presetsC: nextPresets, targetTempC: nextValue }
    }
    if (interaction.key === 'down' && interaction.gesture === 'short') {
      const nextPresets = [...state.presetsC]
      const currentValue = nextPresets[state.selectedPresetIndex]
      if (currentValue == null || currentValue <= 0) {
        nextPresets[state.selectedPresetIndex] = null
        return { ...state, presetsC: nextPresets }
      }
      const nextValue = currentValue - 1
      nextPresets[state.selectedPresetIndex] = nextValue
      return { ...state, presetsC: nextPresets, targetTempC: nextValue }
    }
    if (
      (interaction.key === 'left' && interaction.gesture === 'short') ||
      (interaction.key === 'center' &&
        (interaction.gesture === 'short' || interaction.gesture === 'long'))
    ) {
      return { ...state, route: 'menu' }
    }
    return state
  }

  if (state.route === 'active-cooling') {
    if (interaction.key === 'right' && interaction.gesture === 'short') {
      const nextMode: Record<CoolingMode, CoolingMode> = {
        smart: 'boost',
        boost: 'off',
        off: 'smart',
      }
      return { ...state, activeCoolingMode: nextMode[state.activeCoolingMode] }
    }
    if (
      (interaction.key === 'up' && interaction.gesture === 'short') ||
      (interaction.key === 'down' && interaction.gesture === 'short')
    ) {
      return { ...state, activeCoolingEnabled: !state.activeCoolingEnabled }
    }
    if (
      (interaction.key === 'left' && interaction.gesture === 'short') ||
      (interaction.key === 'center' &&
        (interaction.gesture === 'short' || interaction.gesture === 'long'))
    ) {
      return { ...state, route: 'menu' }
    }
    return state
  }

  if (
    (interaction.key === 'left' && interaction.gesture === 'short') ||
    (interaction.key === 'center' &&
      (interaction.gesture === 'short' || interaction.gesture === 'long'))
  ) {
    return { ...state, route: 'menu' }
  }

  return state
}

export function frontPanelRuntimeToScreen(state: FrontPanelRuntimeState): FrontPanelScreen {
  if (state.route === 'key-test') {
    return {
      kind: 'key-test',
      title: 'Key Test',
      subtitle: 'Five-way mapping + short / double / long diagnostics',
      activeKey: state.keyTest.activeKey,
      activeGesture: state.keyTest.activeGesture,
      rawKeyLabel: state.keyTest.rawKeyLabel,
      logicalKeyLabel: state.keyTest.logicalKeyLabel,
      gestureLabel: state.keyTest.gestureLabel,
      rawMaskLabel: state.keyTest.rawMaskLabel,
    } satisfies FrontPanelKeyTestScreen
  }

  if (state.route === 'dashboard') {
    return {
      kind: 'dashboard',
      title: 'Dashboard',
      subtitle: 'Up/down ±1°C · center short heat · center double fan · center long menu',
      targetTempC: state.targetTempC,
      heaterEnabled: state.heaterEnabled,
      fanEnabled: state.fanEnabled,
      temperatureThresholdsC: frontPanelDefaultThresholdsC,
    } satisfies FrontPanelDashboardScreen
  }

  if (state.route === 'menu') {
    return {
      kind: 'menu',
      title: 'Menu',
      subtitle: 'Left/right move · center enter · center long dashboard',
      selectedItem: state.selectedMenuItem,
      items: menuItems,
    }
  }

  if (state.route === 'preset-temp') {
    return {
      kind: 'preset-temp',
      title: 'Preset Temp',
      subtitle: 'All preset slots · right next · up/down set value or ---',
      selectedPresetIndex: state.selectedPresetIndex,
      presetsC: state.presetsC,
      temperatureThresholdsC: frontPanelDefaultThresholdsC,
    }
  }

  if (state.route === 'active-cooling') {
    return {
      kind: 'active-cooling',
      title: 'Active Cooling',
      subtitle: 'Up/down toggle · right mode · center/left back',
      enabled: state.activeCoolingEnabled,
      mode: state.activeCoolingMode,
    }
  }

  if (state.route === 'wifi-info') {
    return {
      kind: 'wifi-info',
      title: 'WiFi Info',
      subtitle: 'Readonly · center/left back',
      ssid: 'FluxLab',
      rssiDbm: -58,
      ipAddress: '192.168.4.1',
    }
  }

  return {
    kind: 'device-info',
    title: 'Device Info',
    subtitle: 'Readonly · center/left back',
    board: 'FP-S3',
    firmwareVersion: 'v0.3.0',
    serial: 'S3-001',
  }
}

export function buildRuntimeScreenSnapshot(
  mode: FrontPanelRuntimeMode,
  interactions: ReadonlyArray<FrontPanelRuntimeInteraction> = []
) {
  return interactions.reduce(
    (state, interaction) => applyFrontPanelInteraction(state, interaction),
    createFrontPanelRuntimeState(mode)
  )
}
