import { frontPanelDefaultThresholdsC } from './design-tokens'
import type {
  FanDisplayState,
  FrontPanelDashboardScreen,
  FrontPanelKeyId,
  FrontPanelKeyTestScreen,
  FrontPanelScreen,
  HeaterLockReason,
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
  rawKey?: FrontPanelKeyId
}

export interface FrontPanelRuntimeState {
  mode: FrontPanelRuntimeMode
  route: FrontPanelRoute
  elapsedMs: number
  currentTempC: number
  currentTempDeciC: number
  targetTempC: number
  heaterEnabled: boolean
  heaterOutputPercent: number
  fanRuntimeEnabled: boolean
  fanDisplayState: FanDisplayState
  selectedMenuItem: MenuItemId
  selectedPresetIndex: number
  presetsC: ReadonlyArray<number | null>
  activeCoolingEnabled: boolean
  activeCoolingCooldownEndsAtMs: number | null
  pdContractMv: number
  coolingDisabledLockLatched: boolean
  coolingDisabledLockArmed: boolean
  heaterLockReason: HeaterLockReason | null
  dashboardWarningVisible: boolean
  keyTest: {
    activeKey: FrontPanelKeyId | null
    activeGesture: KeyGestureId | null
    rawKeyLabel: string
    logicalKeyLabel: string
    gestureLabel: string
    rawMaskLabel: string
  }
}

const AUTO_COOLING_MIN_TEMP_C = 40
const AUTO_COOLING_FULL_TEMP_C = 60
const AUTO_COOLING_FAN_COOLDOWN_MS = 30_000
const COOLING_DISABLED_PULSE_START_TEMP_C = 100
const COOLING_DISABLED_HEATER_LOCK_TEMP_C = 350
const COOLING_DISABLED_FAN_FULL_TEMP_C = 360
const HARD_OVERTEMP_TEMP_C = 420
const DASHBOARD_WARNING_BLINK_HALF_PERIOD_MS = 500
const FAN_PULSE_PERIOD_MS = 10_000
const DEFAULT_HEATER_OUTPUT_PERCENT = 18

function resolveDefaultPdContractMv() {
  const rawValue = import.meta.env.VITE_FRONTPANEL_PD_CONTRACT_MV
  const parsed = Number.parseInt(rawValue ?? '', 10)
  if (parsed === 12_000 || parsed === 20_000 || parsed === 28_000) {
    return parsed
  }
  return 20_000
}

const DEFAULT_PD_CONTRACT_MV = resolveDefaultPdContractMv()

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

const rawToLogicalKeyMap: Record<FrontPanelKeyId, FrontPanelKeyId> = {
  center: 'center',
  right: 'right',
  down: 'left',
  left: 'down',
  up: 'up',
}

const logicalToRawKeyMap = Object.fromEntries(
  Object.entries(rawToLogicalKeyMap).map(([rawKey, logicalKey]) => [logicalKey, rawKey])
) as Record<FrontPanelKeyId, FrontPanelKeyId>

const rawLabelMap: Record<FrontPanelKeyId, string> = {
  center: 'CTR',
  right: 'R',
  down: 'D',
  left: 'L',
  up: 'U',
}

const logicalLabelMap: Record<FrontPanelKeyId, string> = {
  center: 'CTR',
  right: 'R',
  down: 'D',
  left: 'L',
  up: 'U',
}

function clampTargetTemp(targetTempC: number) {
  return Math.min(400, Math.max(0, targetTempC))
}

function matchingPresetIndex(state: FrontPanelRuntimeState, targetTempC: number) {
  if (state.presetsC[state.selectedPresetIndex] === targetTempC) {
    return state.selectedPresetIndex
  }

  const index = state.presetsC.indexOf(targetTempC)
  return index >= 0 ? index : null
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

function nextPresetSlot(state: FrontPanelRuntimeState) {
  if (!state.presetsC.length) return null
  const nextIndex = (state.selectedPresetIndex + 1) % state.presetsC.length
  return { index: nextIndex, tempC: state.presetsC[nextIndex] }
}

function updateKeyTest(
  state: FrontPanelRuntimeState,
  interaction: FrontPanelRuntimeInteraction
): FrontPanelRuntimeState {
  const logicalKey = interaction.key
  const rawKey = interaction.rawKey ?? logicalToRawKeyMap[logicalKey]

  return {
    ...state,
    keyTest: {
      activeKey: logicalKey,
      activeGesture: interaction.gesture,
      rawKeyLabel: rawLabelMap[rawKey],
      logicalKeyLabel: logicalLabelMap[logicalKey],
      gestureLabel: interaction.gesture.toUpperCase(),
      rawMaskLabel: keyMaskMap[rawKey],
    },
  }
}

function reconcileCoolingDisabledLock(state: FrontPanelRuntimeState) {
  if (state.activeCoolingEnabled) {
    return {
      coolingDisabledLockLatched: false,
      coolingDisabledLockArmed: true,
    }
  }

  if (state.currentTempC <= COOLING_DISABLED_HEATER_LOCK_TEMP_C) {
    return {
      coolingDisabledLockLatched: state.coolingDisabledLockLatched,
      coolingDisabledLockArmed: true,
    }
  }

  if (state.coolingDisabledLockArmed) {
    return {
      coolingDisabledLockLatched: true,
      coolingDisabledLockArmed: false,
    }
  }

  return {
    coolingDisabledLockLatched: state.coolingDisabledLockLatched,
    coolingDisabledLockArmed: state.coolingDisabledLockArmed,
  }
}

function coolingDisabledPulseDutyPercent(currentTempC: number) {
  if (currentTempC <= COOLING_DISABLED_PULSE_START_TEMP_C) {
    return 0
  }

  return Math.min(25, Math.floor((currentTempC - COOLING_DISABLED_PULSE_START_TEMP_C) / 10))
}

function coolingDisabledPulseEnabled(state: FrontPanelRuntimeState) {
  const dutyPercent = coolingDisabledPulseDutyPercent(state.currentTempC)
  if (dutyPercent <= 0) {
    return false
  }

  const elapsedInPeriodMs = state.elapsedMs % FAN_PULSE_PERIOD_MS
  const onWindowMs = Math.floor((FAN_PULSE_PERIOD_MS * dutyPercent) / 100)
  return elapsedInPeriodMs < onWindowMs
}

function reconcileCoolingState(state: FrontPanelRuntimeState): FrontPanelRuntimeState {
  const coolingDisabledLock = reconcileCoolingDisabledLock(state)
  const hardOvertempLatched =
    state.currentTempC >= HARD_OVERTEMP_TEMP_C || state.heaterLockReason === 'hard-overtemp'
  const heaterLockReason: HeaterLockReason | null = hardOvertempLatched
    ? 'hard-overtemp'
    : coolingDisabledLock.coolingDisabledLockLatched
      ? 'cooling-disabled-overtemp'
      : null
  let fanRuntimeEnabled = state.fanRuntimeEnabled
  let fanDisplayState: FanDisplayState = state.activeCoolingEnabled ? 'auto' : 'off'
  let heaterEnabled = state.heaterEnabled
  let activeCoolingCooldownEndsAtMs = state.activeCoolingCooldownEndsAtMs

  if (heaterEnabled) {
    activeCoolingCooldownEndsAtMs = null
    if (state.currentTempC > COOLING_DISABLED_FAN_FULL_TEMP_C) {
      fanRuntimeEnabled = true
    } else if (state.currentTempC > COOLING_DISABLED_HEATER_LOCK_TEMP_C) {
      fanRuntimeEnabled = true
    } else if (state.currentTempC > COOLING_DISABLED_PULSE_START_TEMP_C) {
      fanRuntimeEnabled = coolingDisabledPulseEnabled(state)
    } else {
      fanRuntimeEnabled = false
    }
    fanDisplayState = state.activeCoolingEnabled ? (fanRuntimeEnabled ? 'run' : 'auto') : 'off'
  } else if (state.activeCoolingEnabled) {
    if (state.currentTempC > AUTO_COOLING_FULL_TEMP_C) {
      fanRuntimeEnabled = true
      activeCoolingCooldownEndsAtMs = null
    } else if (state.currentTempC >= AUTO_COOLING_MIN_TEMP_C) {
      fanRuntimeEnabled = true
      activeCoolingCooldownEndsAtMs = null
    } else if (
      activeCoolingCooldownEndsAtMs != null &&
      state.elapsedMs < activeCoolingCooldownEndsAtMs
    ) {
      fanRuntimeEnabled = true
    } else if (state.fanRuntimeEnabled) {
      fanRuntimeEnabled = true
      activeCoolingCooldownEndsAtMs = state.elapsedMs + AUTO_COOLING_FAN_COOLDOWN_MS
    } else {
      fanRuntimeEnabled = false
      activeCoolingCooldownEndsAtMs = null
    }
    fanDisplayState = fanRuntimeEnabled ? 'run' : 'auto'
  } else {
    activeCoolingCooldownEndsAtMs = null
    if (state.currentTempC > AUTO_COOLING_FULL_TEMP_C) {
      fanRuntimeEnabled = true
    } else if (state.currentTempC > COOLING_DISABLED_HEATER_LOCK_TEMP_C) {
      fanRuntimeEnabled = true
    } else if (state.currentTempC > COOLING_DISABLED_PULSE_START_TEMP_C) {
      fanRuntimeEnabled = coolingDisabledPulseEnabled(state)
    } else {
      fanRuntimeEnabled = false
    }
    fanDisplayState = 'off'
  }

  if (heaterLockReason) {
    heaterEnabled = false
  }

  return {
    ...state,
    heaterEnabled,
    heaterOutputPercent: heaterEnabled ? state.heaterOutputPercent : 0,
    fanRuntimeEnabled,
    fanDisplayState,
    activeCoolingCooldownEndsAtMs,
    coolingDisabledLockLatched: coolingDisabledLock.coolingDisabledLockLatched,
    coolingDisabledLockArmed: coolingDisabledLock.coolingDisabledLockArmed,
    heaterLockReason,
    dashboardWarningVisible:
      heaterLockReason != null &&
      Math.floor(state.elapsedMs / DASHBOARD_WARNING_BLINK_HALF_PERIOD_MS) % 2 === 0,
  }
}

export function createFrontPanelRuntimeState(
  mode: FrontPanelRuntimeMode = 'app'
): FrontPanelRuntimeState {
  return reconcileCoolingState({
    mode,
    route: mode === 'key-test' ? 'key-test' : 'dashboard',
    elapsedMs: 0,
    currentTempC: 32,
    currentTempDeciC: 321,
    targetTempC: 100,
    heaterEnabled: false,
    heaterOutputPercent: 0,
    fanRuntimeEnabled: false,
    fanDisplayState: 'auto',
    selectedMenuItem: 'active-cooling',
    selectedPresetIndex: 1,
    presetsC: [50, 100, 120, 150, 180, 200, 210, 220, 250, 300],
    activeCoolingEnabled: true,
    activeCoolingCooldownEndsAtMs: null,
    pdContractMv: DEFAULT_PD_CONTRACT_MV,
    coolingDisabledLockLatched: false,
    coolingDisabledLockArmed: true,
    heaterLockReason: null,
    dashboardWarningVisible: false,
    keyTest: {
      activeKey: null,
      activeGesture: null,
      rawKeyLabel: '---',
      logicalKeyLabel: '---',
      gestureLabel: 'IDLE',
      rawMaskLabel: 'MASK 00',
    },
  })
}

export function tickFrontPanelRuntime(
  current: FrontPanelRuntimeState,
  elapsedMsDelta: number
): FrontPanelRuntimeState {
  return reconcileCoolingState({
    ...current,
    elapsedMs: current.elapsedMs + Math.max(0, elapsedMsDelta),
  })
}

export function applyFrontPanelInteraction(
  current: FrontPanelRuntimeState,
  interaction: FrontPanelRuntimeInteraction
): FrontPanelRuntimeState {
  const state = updateKeyTest(current, interaction)
  if (state.mode === 'key-test') return state

  if (state.route === 'dashboard') {
    if (
      interaction.key === 'up' &&
      (interaction.gesture === 'short' || interaction.gesture === 'repeat')
    ) {
      const targetTempC = clampTargetTemp(state.targetTempC + 1)
      return reconcileCoolingState({
        ...state,
        targetTempC,
        selectedPresetIndex: matchingPresetIndex(state, targetTempC) ?? state.selectedPresetIndex,
      })
    }
    if (
      interaction.key === 'down' &&
      (interaction.gesture === 'short' || interaction.gesture === 'repeat')
    ) {
      const targetTempC = clampTargetTemp(state.targetTempC - 1)
      return reconcileCoolingState({
        ...state,
        targetTempC,
        selectedPresetIndex: matchingPresetIndex(state, targetTempC) ?? state.selectedPresetIndex,
      })
    }
    if (interaction.key === 'left' && interaction.gesture === 'short') {
      const neighbor = findNeighborPreset(state, false)
      return neighbor
        ? reconcileCoolingState({
            ...state,
            targetTempC: neighbor.tempC,
            selectedPresetIndex: neighbor.index,
          })
        : state
    }
    if (interaction.key === 'right' && interaction.gesture === 'short') {
      const neighbor = findNeighborPreset(state, true)
      return neighbor
        ? reconcileCoolingState({
            ...state,
            targetTempC: neighbor.tempC,
            selectedPresetIndex: neighbor.index,
          })
        : state
    }
    if (interaction.key === 'center' && interaction.gesture === 'short') {
      const heaterEnabled = !state.heaterEnabled
      const clearsHardOvertempLock =
        heaterEnabled &&
        state.heaterLockReason === 'hard-overtemp' &&
        state.currentTempC < HARD_OVERTEMP_TEMP_C
      const blocksHardOvertempRearm =
        heaterEnabled &&
        state.heaterLockReason === 'hard-overtemp' &&
        state.currentTempC >= HARD_OVERTEMP_TEMP_C
      const lockOverrideState =
        heaterEnabled && state.heaterLockReason === 'cooling-disabled-overtemp'
          ? {
              coolingDisabledLockLatched: false,
              coolingDisabledLockArmed: false,
            }
          : {}
      return reconcileCoolingState({
        ...state,
        ...lockOverrideState,
        heaterEnabled: blocksHardOvertempRearm ? false : heaterEnabled,
        heaterOutputPercent:
          blocksHardOvertempRearm || !heaterEnabled ? 0 : DEFAULT_HEATER_OUTPUT_PERCENT,
        heaterLockReason: clearsHardOvertempLock ? null : state.heaterLockReason,
      })
    }
    if (interaction.key === 'center' && interaction.gesture === 'double') {
      return reconcileCoolingState({
        ...state,
        activeCoolingEnabled: !state.activeCoolingEnabled,
      })
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
      const entry = nextPresetSlot(state)
      return entry ? { ...state, selectedPresetIndex: entry.index } : state
    }
    if (
      interaction.key === 'up' &&
      (interaction.gesture === 'short' || interaction.gesture === 'repeat')
    ) {
      const nextPresets = [...state.presetsC]
      const currentValue = nextPresets[state.selectedPresetIndex]
      const nextValue = clampTargetTemp(currentValue == null ? 0 : currentValue + 1)
      nextPresets[state.selectedPresetIndex] = nextValue
      return reconcileCoolingState({ ...state, presetsC: nextPresets, targetTempC: nextValue })
    }
    if (
      interaction.key === 'down' &&
      (interaction.gesture === 'short' || interaction.gesture === 'repeat')
    ) {
      const nextPresets = [...state.presetsC]
      const currentValue = nextPresets[state.selectedPresetIndex]
      if (currentValue == null || currentValue <= 0) {
        nextPresets[state.selectedPresetIndex] = null
        return { ...state, presetsC: nextPresets }
      }
      const nextValue = clampTargetTemp(currentValue - 1)
      nextPresets[state.selectedPresetIndex] = nextValue
      return reconcileCoolingState({ ...state, presetsC: nextPresets, targetTempC: nextValue })
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
      subtitle:
        'Up/down ±1°C, hold repeats · center short heat · center double active cooling · center long menu',
      currentTempC: state.currentTempC,
      currentTempDeciC: state.currentTempDeciC,
      targetTempC: state.targetTempC,
      heaterEnabled: state.heaterEnabled,
      heaterOutputPercent: state.heaterOutputPercent,
      fanRuntimeEnabled: state.fanRuntimeEnabled,
      fanDisplayState: state.fanDisplayState,
      pdContractMv: state.pdContractMv,
      heaterLockReason: state.heaterLockReason,
      dashboardWarningVisible: state.dashboardWarningVisible,
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
      subtitle: 'All preset slots · right next · up/down set value or ---, hold repeats',
      selectedPresetIndex: state.selectedPresetIndex,
      presetsC: state.presetsC,
      temperatureThresholdsC: frontPanelDefaultThresholdsC,
    }
  }

  if (state.route === 'active-cooling') {
    return {
      kind: 'active-cooling',
      title: 'Active Cooling',
      subtitle: 'Readonly policy summary · center/left back',
      enabled: state.activeCoolingEnabled,
      pdContractMv: state.pdContractMv,
      cooldownTempC: AUTO_COOLING_MIN_TEMP_C,
      cooldownSeconds: AUTO_COOLING_FAN_COOLDOWN_MS / 1000,
      autoFullTempC: AUTO_COOLING_FULL_TEMP_C,
      pulseStartTempC: COOLING_DISABLED_PULSE_START_TEMP_C,
      lockTempC: COOLING_DISABLED_HEATER_LOCK_TEMP_C,
      fullTempC: COOLING_DISABLED_FAN_FULL_TEMP_C,
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
