import { describe, expect, it } from 'vitest'

import {
  advanceFirmwareRun,
  executionProgress,
  executionStages,
  firmwareStages,
  initialFirmwareRun,
  preflightProgress,
  preflightStages,
  settleReconnectTimeout,
} from './state-machine'

describe('firmware installer state machine', () => {
  it('adds erase only for install/recovery', () => {
    expect(firmwareStages('install_recovery')).toContain('erase')
    expect(firmwareStages('update')).not.toContain('erase')
  })

  it('models preflight and execution as independent progress tracks', () => {
    expect(preflightStages()).toEqual([
      'artifact',
      'transport',
      'rom_reset',
      'chip_flash_security',
      'layout_config',
      'preflight',
    ])
    expect(executionStages('update')).toEqual([
      'authorization',
      'write_segments',
      'rom_md5',
      'reset',
      'runtime_reconnect',
      'runtime_verify',
    ])
    expect(preflightProgress('preflight', 1)).toBe(100)
    expect(executionProgress('update', 'authorization', 0)).toBe(0)
  })

  it('reserves execution 100 percent for the verified terminal outcome', () => {
    expect(executionProgress('update', 'runtime_verify', 1)).toBe(99)
    expect(executionProgress('install_recovery', 'runtime_verify', 1)).toBe(99)
  })

  it('settles a post-write reconnect timeout without claiming success', () => {
    let state = initialFirmwareRun('install_recovery', 'browser')
    while (state.stage !== 'runtime_reconnect') state = advanceFirmwareRun(state, state.stage)
    expect(settleReconnectTimeout(state).outcome).toBe('write_complete_unverified')
  })

  it('does not resume terminal runs', () => {
    const failed = { ...initialFirmwareRun('update', 'devd'), outcome: 'failed' as const }
    expect(() => advanceFirmwareRun(failed, 'retry')).toThrow(/cannot resume/)
  })
})
