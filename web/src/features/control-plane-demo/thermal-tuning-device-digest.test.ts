import { describe, expect, it } from 'vitest'
import type { ThermalTuningEvent } from './contracts'
import { digestEventChain } from './thermal-tuning-device-digest'
import fixture from './thermal-tuning-digest.fixture.json'

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
      '4ea3bc9c23cbf417bbcd6468998d28c493745c28305d92ead01274c3a2f4f9d3'
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
      '97d020d2c3f902b33a1d48b53e684e2c5e93d7bd76eb6138f249ccbc3e51e893'
    )
  })
  it('matches the shared Rust serializer with conditional timing evidence', async () => {
    const events: ThermalTuningEvent[] = JSON.parse(JSON.stringify(fixture))
    expect(await digestEventChain(events)).toBe(
      '76ee2a15a2e7bef6433ff8deb2e401a6ea483fbcbf1a1b0f5ed54e4eac9ed82b'
    )
  })
})
