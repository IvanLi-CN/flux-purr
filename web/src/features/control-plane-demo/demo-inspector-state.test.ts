import { describe, expect, it } from 'vitest'
import {
  defaultDemoInspectorState,
  demoInspectorSearch,
  demoInspectorStateFromSearch,
  deriveDemoScenario,
} from './demo-inspector-state'
import type { EventLogEntry } from './types'
import { resolveWifiSettingsAccess } from './wifi-settings-access'

describe('Demo Inspector state', () => {
  it('normalizes invalid share state to deterministic defaults', () => {
    expect(
      demoInspectorStateFromSearch({
        demoScene: 'unknown',
        demoLease: 'busy',
        demoNetwork: 'offline',
        demoArtifact: 'invalid',
        demoWifiCapability: 'unknown',
        demoLogs: 'verbose',
        demoTelemetry: 'missing',
      })
    ).toEqual(defaultDemoInspectorState)
  })

  it('serializes only non-default share state', () => {
    expect(
      demoInspectorSearch({
        ...defaultDemoInspectorState,
        demoScene: 'degraded',
        demoLease: 'conflict',
        demoNetwork: 'timeout',
        demoArtifact: 'blocked',
        demoWifiCapability: 'read-only',
        demoLogs: 'empty',
        demoTelemetry: 'unavailable',
      })
    ).toEqual({
      demoScene: 'degraded',
      demoLease: 'conflict',
      demoNetwork: 'timeout',
      demoArtifact: 'blocked',
      demoWifiCapability: 'read-only',
      demoLogs: 'empty',
      demoTelemetry: 'unavailable',
    })
  })

  it('applies faults to the routed target only', () => {
    const scenario = deriveDemoScenario(
      { ...defaultDemoInspectorState, demoLease: 'conflict', demoNetwork: 'timeout' },
      [],
      [],
      'fp-kit-02'
    )
    const routedTarget = scenario.devices.find((device) => device.id === 'fp-kit-02')
    const untouchedTarget = scenario.devices.find((device) => device.id === 'fp-lab-01')

    expect(scenario.selectedDeviceId).toBe('fp-kit-02')
    expect(routedTarget).toMatchObject({
      leaseState: 'conflict',
      networkState: 'timeout',
      wifiRssi: null,
    })
    expect(untouchedTarget).toMatchObject({ leaseState: 'none', networkState: 'connected' })
  })

  it('projects thermal safety over recovery actions', () => {
    const thermalThenRecovery = deriveDemoScenario(
      defaultDemoInspectorState,
      [],
      [
        { kind: 'thermal-warning', targetDeviceId: 'fp-lab-01', sequence: 1 },
        { kind: 'fan-recovery', targetDeviceId: 'fp-lab-01', sequence: 2 },
      ],
      'fp-lab-01'
    )
    const recoveryOnly = deriveDemoScenario(
      defaultDemoInspectorState,
      [],
      [{ kind: 'fan-recovery', targetDeviceId: 'fp-lab-01', sequence: 1 }],
      'fp-lab-01'
    )

    expect(thermalThenRecovery.devices[0]).toMatchObject({
      heaterLockReason: 'hard-overtemp',
      heaterEnabled: false,
      fanState: 'SAFE',
      fanPolicySource: 'safety',
      currentTempC: 420,
    })
    expect(recoveryOnly.devices[0]).toMatchObject({
      fanState: 'AUTO',
      fanDisplayState: 'AUTO',
      fanPolicySource: 'post_heat',
    })
  })

  it('keeps data controls scoped and preserves action events in an empty log', () => {
    const actionEvent: EventLogEntry = {
      time: '20:20:10',
      source: 'demo',
      message: 'simulated action receipt',
      tone: 'success',
    }
    const scenario = deriveDemoScenario(
      {
        ...defaultDemoInspectorState,
        demoWifiCapability: 'unavailable',
        demoLogs: 'empty',
        demoTelemetry: 'unavailable',
      },
      [actionEvent],
      [],
      'fp-lab-01'
    )
    const target = scenario.devices.find((device) => device.id === 'fp-lab-01')

    expect(target).toMatchObject({
      capabilities: expect.not.arrayContaining(['wifi_config', 'wifi_state_v2']),
      wifiSsid: null,
      wifiRssi: null,
      wifiPasswordLength: 0,
      networkState: 'disabled',
      currentTempC: -1,
      boardTempC: -1,
    })
    expect(scenario.events).toEqual([actionEvent])
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

  it('exposes a writable USB fixture for the public demo', () => {
    const scenario = deriveDemoScenario(defaultDemoInspectorState)
    const fixture = scenario.devices.find((device) => device.id === 'fp-wifi-usb-04')

    expect(fixture).toBeDefined()
    expect(fixture && resolveWifiSettingsAccess(fixture)).toEqual({ mode: 'read-write' })
  })

  it('keeps the read-only mock snapshot visible with SSID metadata', () => {
    const scenario = deriveDemoScenario(defaultDemoInspectorState)
    const fixture = scenario.devices.find((device) => device.id === 'fp-lab-01')

    expect(fixture).toMatchObject({
      wifiSsid: 'FluxPurr-Lab',
      wifiRssi: -54,
      wifiPasswordLength: 12,
    })
  })
})
