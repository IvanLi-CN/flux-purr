import type { FirmwareOperation, FirmwareRunState, FirmwareStage, FirmwareTransport } from './types'

const BASE_STAGES: FirmwareStage[] = [
  'artifact',
  'transport',
  'rom_reset',
  'chip_flash_security',
  'layout_config',
  'preflight',
]
const WRITE_STAGES: FirmwareStage[] = [
  'write_segments',
  'rom_md5',
  'reset',
  'runtime_reconnect',
  'runtime_verify',
]

export function firmwareStages(operation: FirmwareOperation): FirmwareStage[] {
  return [
    ...BASE_STAGES,
    ...(operation === 'install_recovery' ? (['erase'] as const) : []),
    ...WRITE_STAGES,
  ]
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
