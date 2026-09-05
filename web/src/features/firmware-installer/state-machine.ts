import type { FirmwareOperation, FirmwareRunState, FirmwareStage, FirmwareTransport } from './types'

export const PREFLIGHT_STAGES = [
  'artifact',
  'transport',
  'rom_reset',
  'chip_flash_security',
  'preflight',
] as const satisfies readonly FirmwareStage[]

const UPDATE_EXECUTION_STAGES = [
  'authorization',
  'write_segments',
  'rom_md5',
  'reset',
  'runtime_reconnect',
  'runtime_verify',
] as const satisfies readonly FirmwareStage[]

const RECOVERY_EXECUTION_STAGES = [
  'authorization',
  'erase',
  ...UPDATE_EXECUTION_STAGES.slice(1),
] as const satisfies readonly FirmwareStage[]

export type FirmwarePreflightStage = (typeof PREFLIGHT_STAGES)[number]
export type FirmwareExecutionStage = (typeof RECOVERY_EXECUTION_STAGES)[number]

export function preflightStages(): FirmwarePreflightStage[] {
  return [...PREFLIGHT_STAGES]
}

export function executionStages(operation: FirmwareOperation): FirmwareExecutionStage[] {
  return [
    ...(operation === 'install_recovery' ? RECOVERY_EXECUTION_STAGES : UPDATE_EXECUTION_STAGES),
  ]
}

export function firmwareStages(operation: FirmwareOperation): FirmwareStage[] {
  return [...preflightStages(), ...executionStages(operation)]
}

export function preflightProgress(stage: FirmwarePreflightStage, stageProgress = 0): number {
  const index = PREFLIGHT_STAGES.indexOf(stage)
  return phaseProgress(index, PREFLIGHT_STAGES.length, stageProgress, false)
}

export function executionProgress(
  operation: FirmwareOperation,
  stage: FirmwareExecutionStage,
  stageProgress = 0
): number {
  const weights = executionStageWeights(operation)
  const index = weights.findIndex(([candidate]) => candidate === stage)
  if (index < 0) return 0
  const completedWeight = weights.slice(0, index).reduce((total, [, weight]) => total + weight, 0)
  const currentWeight = weights[index][1]
  const progress = completedWeight + currentWeight * clampUnit(stageProgress)
  // 100% is reserved for the terminal `verified` outcome.
  return Math.min(99, Math.round(progress * 100))
}

function executionStageWeights(
  operation: FirmwareOperation
): Array<readonly [FirmwareExecutionStage, number]> {
  if (operation === 'install_recovery') {
    return [
      ['authorization', 0.04],
      ['erase', 0.12],
      ['write_segments', 0.46],
      ['rom_md5', 0.12],
      ['reset', 0.06],
      ['runtime_reconnect', 0.08],
      ['runtime_verify', 0.12],
    ]
  }
  return [
    ['authorization', 0.04],
    ['write_segments', 0.58],
    ['rom_md5', 0.14],
    ['reset', 0.06],
    ['runtime_reconnect', 0.08],
    ['runtime_verify', 0.1],
  ]
}

function phaseProgress(
  stageIndex: number,
  stageCount: number,
  stageProgress: number,
  reserveTerminal: boolean
) {
  if (stageIndex < 0 || stageCount <= 0) return 0
  const progress = ((stageIndex + clampUnit(stageProgress)) / stageCount) * 100
  return Math.min(reserveTerminal ? 99 : 100, Math.round(progress))
}

function clampUnit(value: number) {
  return Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0))
}

export function initialFirmwareRun(
  operation: FirmwareOperation,
  transport: FirmwareTransport
): FirmwareRunState {
  return {
    operation,
    transport,
    stage: 'artifact',
    stageIndex: 0,
    progress: 0,
    outcome: 'idle',
    message: 'Choose a firmware bundle.',
  }
}

export function advanceFirmwareRun(state: FirmwareRunState, message: string): FirmwareRunState {
  if (state.outcome === 'blocked' || state.outcome === 'failed' || state.outcome === 'verified') {
    throw new Error('A terminal firmware run cannot resume; start a new preflight.')
  }
  const stages = firmwareStages(state.operation)
  const nextIndex = state.stageIndex + 1
  if (nextIndex >= stages.length) {
    return { ...state, progress: 100, outcome: 'verified', message }
  }
  return {
    ...state,
    stage: stages[nextIndex],
    stageIndex: nextIndex,
    progress: Math.round((nextIndex / (stages.length - 1)) * 100),
    outcome: 'running',
    message,
  }
}

export function settleReconnectTimeout(state: FirmwareRunState): FirmwareRunState {
  if (state.stage !== 'runtime_reconnect' && state.stage !== 'runtime_verify') {
    throw new Error('Reconnect timeout is valid only after verified writes.')
  }
  return {
    ...state,
    outcome: 'write_complete_unverified',
    message: 'Firmware bytes are verified, but the target runtime did not verify after reset.',
  }
}
