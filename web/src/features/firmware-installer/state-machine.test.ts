import { describe, expect, it } from 'vitest'

import {
  advanceFirmwareRun,
  firmwareStages,
  initialFirmwareRun,
  settleReconnectTimeout,
} from './state-machine'

describe('firmware installer state machine', () => {
  it('adds erase only for install/recovery', () => {
    expect(firmwareStages('install_recovery')).toContain('erase')
    expect(firmwareStages('update')).not.toContain('erase')
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
