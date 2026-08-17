import { controlPlaneScenario, degradedControlPlaneScenario } from './mock-data'
import type { ControlPlaneScenario, EventLogEntry } from './types'

export const demoSceneIds = [
  'normal',
  'degraded',
  'offline',
  'blocked-artifact',
  'calibration-active',
] as const

export type DemoSceneId = (typeof demoSceneIds)[number]
export type DemoLeaseState = 'none' | 'conflict'
export type DemoNetworkState = 'healthy' | 'timeout'
export type DemoArtifactState = 'ready' | 'blocked'

export interface DemoInspectorState {
  demoScene: DemoSceneId
  demoLease: DemoLeaseState
  demoNetwork: DemoNetworkState
  demoArtifact: DemoArtifactState
}

export const defaultDemoInspectorState: DemoInspectorState = {
  demoScene: 'normal',
  demoLease: 'none',
  demoNetwork: 'healthy',
  demoArtifact: 'ready',
}

export function demoInspectorStateFromSearch(search: Record<string, unknown>): DemoInspectorState {
  return {
    demoScene: normalizeDemoScene(search.demoScene),
    demoLease: search.demoLease === 'conflict' ? 'conflict' : 'none',
    demoNetwork: search.demoNetwork === 'timeout' ? 'timeout' : 'healthy',
    demoArtifact: search.demoArtifact === 'blocked' ? 'blocked' : 'ready',
  }
}

export function demoInspectorSearch(state: DemoInspectorState) {
  return {
    demoScene: state.demoScene === 'normal' ? undefined : state.demoScene,
    demoLease: state.demoLease === 'none' ? undefined : state.demoLease,
    demoNetwork: state.demoNetwork === 'healthy' ? undefined : state.demoNetwork,
    demoArtifact: state.demoArtifact === 'ready' ? undefined : state.demoArtifact,
  }
}

export function deriveDemoScenario(
  state: DemoInspectorState,
  actionEvents: readonly EventLogEntry[] = []
): ControlPlaneScenario {
  const base =
    state.demoScene === 'degraded'
      ? degradedControlPlaneScenario
      : state.demoScene === 'offline'
        ? { ...controlPlaneScenario, selectedDeviceId: 'fp-demo-03' }
        : controlPlaneScenario
  const selectedDeviceId =
    state.demoScene === 'offline'
      ? 'fp-demo-03'
      : state.demoScene === 'degraded'
        ? 'fp-kit-02'
        : 'fp-lab-01'

  const devices = base.devices.map((device) => {
    const fixture =
      device.id === 'fp-kit-02'
        ? { ...device, transport: 'mock' as const, baseUrl: 'mock:simulated-serial-fixture' }
        : device
    if (fixture.id !== selectedDeviceId) return fixture

    const next = {
      ...fixture,
      calibration: { ...fixture.calibration, job: { ...fixture.calibration.job } },
    }
    if (state.demoScene === 'calibration-active') {
      next.calibration = {
        ...next.calibration,
        mode: 'thermal_plant',
        ppsEnabled: true,
        ppsMv: 12_000,
        ppsMa: 1_500,
        heaterEnabled: true,
        targetAdcMv: 1_200,
        stable: false,
        stabilityErrorMv: 18,
        job: {
          kind: 'thermal_plant_auto',
          status: 'running',
          progressPercent: 46,
          samplesCollected: 7,
          nextRequestMv: 1_240,
          message: 'Demo calibration sample sequence is active.',
        },
      }
    }
    if (state.demoLease === 'conflict') {
      next.leaseState = 'conflict'
      next.transportIssue = 'Simulated control lease is owned by another console.'
    }
    if (state.demoNetwork === 'timeout') {
      next.networkState = 'timeout'
      next.wifiRssi = null
      next.transportIssue = 'Simulated network handoff timed out without a live request.'
    }
    return next
  })

  const artifacts = base.artifacts.map((artifact, index) =>
    state.demoScene === 'blocked-artifact' || state.demoArtifact === 'blocked'
      ? index === 0
        ? { ...artifact, compatibility: 'blocked' as const }
        : artifact
      : artifact
  )

  const sceneEvent: EventLogEntry | null =
    state.demoScene === 'calibration-active'
      ? {
          time: '20:18:12',
          source: 'demo',
          message: 'simulated calibration sequence armed',
          tone: 'warning',
        }
      : state.demoScene === 'blocked-artifact' || state.demoArtifact === 'blocked'
        ? {
            time: '20:18:12',
            source: 'demo',
            message: 'simulated artifact compatibility gate blocked',
            tone: 'warning',
          }
        : null

  return {
    ...base,
    selectedDeviceId,
    devices,
    artifacts,
    events: [...base.events, ...(sceneEvent ? [sceneEvent] : []), ...actionEvents].slice(-64),
  }
}

function normalizeDemoScene(value: unknown): DemoSceneId {
  return typeof value === 'string' && demoSceneIds.includes(value as DemoSceneId)
    ? (value as DemoSceneId)
    : 'normal'
}
