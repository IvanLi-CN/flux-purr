import { describe, expect, it } from 'vitest'
import type { ThermalTuningEvent, ThermalTuningStatus } from './contracts'
import { recordThermalTuningPage } from './thermal-tuning-recorder'

const event = (sequence: number): ThermalTuningEvent => ({
  sequence,
  elapsedMs: sequence * 50,
  kind: 'phase_transition',
  phase: 'warmup',
  targetC: 60,
  trialIndex: 0,
  sample: null,
  reason: null,
  trial: null,
  decision: null,
})

function page(
  sequences: number[],
  options: { runId?: string; lastSequence?: number; complete?: boolean; gap?: boolean } = {}
): ThermalTuningStatus {
  const lastSequence = options.lastSequence ?? sequences.at(-1) ?? null
  const trace = {
    firstSequence: 0,
    lastSequence,
    complete: options.complete ?? false,
    digest: 'sha256:test',
    gap: options.gap ?? false,
  }
  return {
    phase: 'warmup',
    trialPurpose: 'scout',
    terminalDisposition: 'running',
    reviewDisposition: 'pending',
    powerClass: 'pps5a',
    runId: options.runId ?? 'run-5a',
    candidateId: 'candidate-5a',
    candidateHash: 'sha256:test',
    targetsC: [60],
    targetIndex: 0,
    armed: true,
    sealed: false,
    saveConfirmed: false,
    candidateProfile: null,
    previewReceipt: null,
    trace,
    events: { events: sequences.map(event), nextSequence: sequences.at(-1) ?? null, trace },
    saveReceipt: null,
  }
}

describe('firmware tuning event recorder', () => {
  it('completes only after the final contiguous page is confirmed by the device', () => {
    const first = recordThermalTuningPage(null, page([0, 1]))
    expect(first.acknowledgedThrough).toBe(1)
    const second = recordThermalTuningPage(
      first,
      page([2, 3], { lastSequence: 3, complete: true }),
      1
    )
    expect(second.events.map((item) => item.sequence)).toEqual([0, 1, 2, 3])
    expect(second.acknowledgedThrough).toBe(3)
    expect(second.confirmedThrough).toBe(1)
    expect(second.complete).toBe(false)

    const final = recordThermalTuningPage(second, page([], { lastSequence: 3, complete: true }), 3)
    expect(final.confirmedThrough).toBe(3)
    expect(final.complete).toBe(true)
  })

  it('does not acknowledge a missing or changed event', () => {
    const first = recordThermalTuningPage(null, page([0, 1]))
    const missing = recordThermalTuningPage(first, page([3], { lastSequence: 3 }))
    expect(missing.gapReason).toBe('missing_event_sequence')
    expect(missing.acknowledgedThrough).toBeNull()
    expect(missing.complete).toBe(false)

    const changed = page([1])
    changed.events.events[0].reason = 'changed'
    const conflict = recordThermalTuningPage(first, changed)
    expect(conflict.gapReason).toBe('conflicting_event_sequence')
    expect(conflict.acknowledgedThrough).toBeNull()
  })

  it('starts a new recording when the device run changes', () => {
    const first = recordThermalTuningPage(null, page([0, 1]))
    const next = recordThermalTuningPage(first, page([0], { runId: 'run-next' }))
    expect(next.runId).toBe('run-next')
    expect(next.events.map((item) => item.sequence)).toEqual([0])
  })
})
