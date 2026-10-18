import { describe, expect, it } from 'vitest'
import { type DemoSessionState, defaultDemoSessionState, reduceDemoSession } from './demo-session'

const event = {
  time: '20:19:10',
  source: 'demo',
  message: 'simulated action',
  tone: 'success' as const,
}

describe('DemoSession reducer', () => {
  it('keeps actions and receipts in one bounded session state', () => {
    const next = reduceDemoSession(defaultDemoSessionState, {
      type: 'append-action',
      kind: 'fan-recovery',
      targetDeviceId: 'fp-lab-01',
      event,
    })

    expect(next).toMatchObject({
      actions: [{ kind: 'fan-recovery', targetDeviceId: 'fp-lab-01', sequence: 1 }],
      events: [event],
      generation: 0,
    })
  })

  it('clears transient results and rebuilds the session generation', () => {
    const populated: DemoSessionState = reduceDemoSession(defaultDemoSessionState, {
      type: 'append-action',
      kind: 'thermal-warning',
      targetDeviceId: 'fp-lab-01',
      event,
    })

    expect(reduceDemoSession(populated, { type: 'clear-results' })).toEqual({
      actions: [],
      events: [],
      generation: 1,
    })
  })

  it('increments generation on reset so late work can be discarded', () => {
    const next = reduceDemoSession(defaultDemoSessionState, { type: 'reset' })

    expect(next).toEqual({ actions: [], events: [], generation: 1 })
  })
})
