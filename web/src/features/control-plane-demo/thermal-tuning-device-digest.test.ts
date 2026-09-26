import { describe, expect, it } from 'vitest'
import type { ThermalTuningEvent } from './contracts'
import { digestEventChain } from './thermal-tuning-device-digest'

describe('firmware event digest compatibility', () => {
  it('matches Rust safety and floating-point decision vectors regardless of received key order', async () => {
    const safety: ThermalTuningEvent = {
      sequence: 0,
      elapsedMs: 0,
      kind: 'safety',
      phase: 'failed',
      targetC: null,
      trialIndex: null,
      sample: null,
      reason: 'pps3a_not_supported',
      trial: null,
      decision: null,
    }
    expect(await digestEventChain([safety])).toBe(
      '6c91f9e71b0dd2e548b43b5c3bcebfb52a3834567a05b88469af1ea934122f44'
    )
    const decision: ThermalTuningEvent = {
      ...safety,
      elapsedMs: 4500,
      kind: 'decision',
      phase: 'hold',
      targetC: 60,
      trialIndex: 1,
      reason: 'candidate_adopted',
      decision: {
        stopReason: 'accepted',
        prune: false,
        freeze: true,
        hardGateFullSpeedToStableMs: 12000,
        hardGateHoldPeakToPeakCentiC: 300,
        hardGateOvershootCentiC: 300,
        holdConfirm: true,
        approachToHold: true,
        warmupFull: true,
        score: [0, 1, 1.25, 2, 3.5, 4, 5.25, 6, 7.5, 8, 9.25],
        settleTimeMs: 7800,
        holdPeakToPeakCentiC: 155,
        maxOvershootCentiC: 125,
        purpose: 'hold_confirm',
        disposition: 'adopted',
      },
    }
    expect(await digestEventChain([decision])).toBe(
      '6ce5cc6485ed1f4d4a7d9e35e2ce766386da295efec80563c2eaa1ad47408b75'
    )
  })
})
