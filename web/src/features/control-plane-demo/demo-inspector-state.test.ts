import { describe, expect, it } from 'vitest'
import {
  defaultDemoInspectorState,
  demoInspectorSearch,
  demoInspectorStateFromSearch,
  deriveDemoScenario,
} from './demo-inspector-state'

describe('Demo Inspector state', () => {
  it('normalizes invalid share state to deterministic defaults', () => {
    expect(
      demoInspectorStateFromSearch({
        demoScene: 'unknown',
        demoLease: 'busy',
        demoNetwork: 'offline',
        demoArtifact: 'invalid',
      })
    ).toEqual(defaultDemoInspectorState)
  })

  it('serializes only non-default share state', () => {
    expect(
      demoInspectorSearch({
        demoScene: 'degraded',
        demoLease: 'conflict',
        demoNetwork: 'timeout',
        demoArtifact: 'blocked',
      })
    ).toEqual({
      demoScene: 'degraded',
      demoLease: 'conflict',
      demoNetwork: 'timeout',
      demoArtifact: 'blocked',
    })
  })

  it('derives blocked and calibration fixtures without mutating the baseline', () => {
    const blocked = deriveDemoScenario({ ...defaultDemoInspectorState, demoArtifact: 'blocked' })
    const calibration = deriveDemoScenario({
      ...defaultDemoInspectorState,
      demoScene: 'calibration-active',
    })

    expect(blocked.artifacts[0]?.compatibility).toBe('blocked')
    expect(calibration.selectedDeviceId).toBe('fp-lab-01')
    expect(calibration.devices[0]?.calibration.job.status).toBe('running')
    expect(deriveDemoScenario(defaultDemoInspectorState).artifacts[0]?.compatibility).not.toBe(
      'blocked'
    )
  })

  it('keeps the serial fixture simulated for every public demo scene', () => {
    const degraded = deriveDemoScenario({ ...defaultDemoInspectorState, demoScene: 'degraded' })
    const fixture = degraded.devices.find((device) => device.id === 'fp-kit-02')

    expect(fixture).toMatchObject({
      transport: 'mock',
      baseUrl: 'mock:simulated-serial-fixture',
    })
  })
})
