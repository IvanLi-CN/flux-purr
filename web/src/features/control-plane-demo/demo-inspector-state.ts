import { controlPlaneScenario, degradedControlPlaneScenario } from './mock-data'
import { type ControlPlaneScenario, type EventLogEntry, UNAVAILABLE_TEMPERATURE_C } from './types'

const demoWifiUsbDevice = {
  ...controlPlaneScenario.devices[0],
  id: 'fp-wifi-usb-04',
  identityId: 'fp-wifi-usb-04',
  alias: 'USB Config Fixture',
  location: 'Simulated devd USB',
  transport: 'devd' as const,
  bridgeTransport: 'usb' as const,
  baseUrl: 'devd://demo-wifi-usb',
  connectionAvailable: true,
  leaseState: 'active' as const,
  leaseId: 'demo-wifi-usb-lease',
  capabilities: ['identity', 'status', 'wifi_config', 'wifi_state_v2', 'monitor'],
  wifiSsid: 'Flux Purr Lab',
  wifiRssi: -42,
  wifiPasswordLength: 12,
  networkState: 'connected' as const,
  configurationGeneration: 1,
  transitionSequence: 1,
}

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
export type DemoWifiCapabilityState = 'default' | 'read-only' | 'unavailable'
export type DemoLogState = 'normal' | 'empty'
export type DemoTelemetryState = 'available' | 'unavailable'

export type DemoActionKind = 'thermal-warning' | 'fan-recovery'

export interface DemoAction {
  kind: DemoActionKind
  targetDeviceId: string
  sequence: number
}

export interface DemoInspectorState {
  demoScene: DemoSceneId
  demoLease: DemoLeaseState
  demoNetwork: DemoNetworkState
  demoArtifact: DemoArtifactState
  demoWifiCapability: DemoWifiCapabilityState
  demoLogs: DemoLogState
  demoTelemetry: DemoTelemetryState
}

export const defaultDemoInspectorState: DemoInspectorState = {
  demoScene: 'normal',
  demoLease: 'none',
  demoNetwork: 'healthy',
  demoArtifact: 'ready',
  demoWifiCapability: 'default',
  demoLogs: 'normal',
  demoTelemetry: 'available',
}

export function demoInspectorStateFromSearch(search: Record<string, unknown>): DemoInspectorState {
  return {
    demoScene: normalizeDemoScene(search.demoScene),
    demoLease: search.demoLease === 'conflict' ? 'conflict' : 'none',
    demoNetwork: search.demoNetwork === 'timeout' ? 'timeout' : 'healthy',
    demoArtifact: search.demoArtifact === 'blocked' ? 'blocked' : 'ready',
    demoWifiCapability:
      search.demoWifiCapability === 'read-only' || search.demoWifiCapability === 'unavailable'
        ? search.demoWifiCapability
        : 'default',
    demoLogs: search.demoLogs === 'empty' ? 'empty' : 'normal',
    demoTelemetry: search.demoTelemetry === 'unavailable' ? 'unavailable' : 'available',
  }
}

export function demoInspectorSearch(state: DemoInspectorState) {
  return {
    demoScene: state.demoScene === 'normal' ? undefined : state.demoScene,
    demoLease: state.demoLease === 'none' ? undefined : state.demoLease,
    demoNetwork: state.demoNetwork === 'healthy' ? undefined : state.demoNetwork,
    demoArtifact: state.demoArtifact === 'ready' ? undefined : state.demoArtifact,
    demoWifiCapability:
      state.demoWifiCapability === 'default' ? undefined : state.demoWifiCapability,
    demoLogs: state.demoLogs === 'normal' ? undefined : state.demoLogs,
    demoTelemetry: state.demoTelemetry === 'available' ? undefined : state.demoTelemetry,
  }
}

export function deriveDemoScenario(
  state: DemoInspectorState,
  actionEvents: readonly EventLogEntry[] = [],
  actions: readonly DemoAction[] = [],
  routedDeviceId?: string
): ControlPlaneScenario {
  const base =
    state.demoScene === 'degraded'
      ? degradedControlPlaneScenario
      : state.demoScene === 'offline'
        ? { ...controlPlaneScenario, selectedDeviceId: 'fp-demo-03' }
        : controlPlaneScenario
  const defaultSelectedDeviceId =
    state.demoScene === 'offline'
      ? 'fp-demo-03'
      : state.demoScene === 'degraded'
        ? 'fp-kit-02'
        : 'fp-lab-01'
  const baseDeviceIds = new Set([...base.devices.map((device) => device.id), demoWifiUsbDevice.id])
  const selectedDeviceId =
    routedDeviceId && baseDeviceIds.has(routedDeviceId) ? routedDeviceId : defaultSelectedDeviceId

  const devices = [...base.devices, demoWifiUsbDevice].map((device) => {
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
    if (state.demoWifiCapability === 'read-only') {
      next.capabilities = next.capabilities.filter((capability) => capability !== 'wifi_config')
    } else if (state.demoWifiCapability === 'unavailable') {
      next.capabilities = next.capabilities.filter(
        (capability) => capability !== 'wifi_config' && capability !== 'wifi_state_v2'
      )
      next.wifiSsid = null
      next.wifiRssi = null
      next.wifiPasswordLength = 0
      next.networkState = 'disabled'
    }
    if (state.demoTelemetry === 'unavailable') {
      next.currentTempC = UNAVAILABLE_TEMPERATURE_C
      next.boardTempC = UNAVAILABLE_TEMPERATURE_C
      next.voltageMv = 0
      next.currentMa = 0
    }
    for (const action of actions) {
      if (action.targetDeviceId !== selectedDeviceId) continue
      if (action.kind === 'thermal-warning') {
        next.heaterLockReason = 'hard-overtemp'
        next.faultAttentionPending = true
        next.heaterEnabled = false
        next.heaterOutputPercent = 0
        next.activeCoolingEnabled = true
        next.fanState = 'SAFE'
        next.fanDisplayState = 'SAFE'
        next.fanPolicySource = 'safety'
        next.fanOutputLevel = 'limited'
        if (state.demoTelemetry === 'available') next.currentTempC = 420
      } else if (action.kind === 'fan-recovery' && !next.heaterLockReason) {
        next.fanState = 'AUTO'
        next.fanDisplayState = 'AUTO'
        next.fanPolicySource = 'post_heat'
        next.fanOutputLevel = 'medium'
      }
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
    events: [
      ...(state.demoLogs === 'empty' ? [] : base.events),
      ...(sceneEvent ? [sceneEvent] : []),
      ...actionEvents,
    ].slice(-64),
  }
}

function normalizeDemoScene(value: unknown): DemoSceneId {
  return typeof value === 'string' && demoSceneIds.includes(value as DemoSceneId)
    ? (value as DemoSceneId)
    : 'normal'
}
