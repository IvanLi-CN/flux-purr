import { describe, expect, it } from 'vitest'
import type { ThermalTuningEvent } from './contracts'
import { digestEventChain } from './thermal-tuning-device-digest'
import fixture from './thermal-tuning-digest.fixture.json'
import fanFixture from './thermal-tuning-fan.fixture.json'

describe('firmware event digest compatibility', () => {
  it('binds the model floor while retaining the unused candidate floor identity', async () => {
    const events: ThermalTuningEvent[] = JSON.parse(JSON.stringify(fixture))
    const timing = events[1].decision?.timing
    if (!timing || typeof timing !== 'object') throw new Error('missing timing fixture')
    Object.assign(timing, {
      policyVersion: 3,
      configuredApproachFloorPermille: 420,
      modelApproachFloorPermille: 100,
      originalDeadlineObservedAtMs: 11_000,
      originalDeadlinePhase: 'approach',
    })
    const expected = '598383346ab2f5d316bd0e9358ff7a1bcce4503dddc317bfb5df6e36773aa01a'
    expect(await digestEventChain(events)).toBe(expected)
    Object.assign(timing, { modelApproachFloorPermille: 200 })
    expect(await digestEventChain(events)).not.toBe(expected)
  })
  it('matches Rust fan sample bytes and binds the applied fan command', async () => {
    const event: ThermalTuningEvent = JSON.parse(JSON.stringify(fanFixture))
    const expected = '966de02dc5498cccd6d899bb359104a1e1e83a0ec4d7dca65da7fd9dc6546ea6'
    expect(await digestEventChain([event])).toBe(expected)
    if (!event.sample?.fan) throw new Error('missing fan sample fixture')
    event.sample.fan.enabled = false
    expect(await digestEventChain([event])).not.toBe(expected)
  })
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
  it('matches Rust policy 2 bytes without changing historical policy 1 digests', async () => {
    const events: ThermalTuningEvent[] = JSON.parse(JSON.stringify(fixture))
    const timing = events[1].decision?.timing
    if (!timing || typeof timing !== 'object') throw new Error('missing timing fixture')
    Object.assign(timing, {
      policyVersion: 2,
      configuredApproachFloorPermille: 100,
      originalDeadlineObservedAtMs: 11_000,
      originalDeadlinePhase: 'approach',
    })
    expect(await digestEventChain(events)).toBe(
      'ed0fa77d928f38f1b346a5f5d1c31737a590ba1ea53e06eacd5c468988ef27fb'
    )
    Object.assign(timing, { originalDeadlinePhase: 'hold' })
    expect(await digestEventChain(events)).not.toBe(
      'ed0fa77d928f38f1b346a5f5d1c31737a590ba1ea53e06eacd5c468988ef27fb'
    )
  })
})
