import type { AppVariant } from '@/app-mode'
import {
  type DemoArtifactState,
  type DemoLeaseState,
  type DemoNetworkState,
  type DemoSceneId,
  demoInspectorSearch,
  demoInspectorStateFromSearch,
} from '@/features/control-plane-demo/demo-inspector-state'
import { isPublicDemoBuild } from '@/public-demo'

export interface AppSearch {
  demo?: boolean
  uiDemo?: true
  demoScene?: DemoSceneId
  demoLease?: DemoLeaseState
  demoNetwork?: DemoNetworkState
  demoArtifact?: DemoArtifactState
  demoWifiCapability?: 'default' | 'read-only' | 'unavailable'
  demoLogs?: 'normal' | 'empty'
  demoTelemetry?: 'available' | 'unavailable'
}

export function validateAppSearch(search: Record<string, unknown>): AppSearch {
  const demoBuild = isPublicDemoBuild()
  const demoState = demoInspectorStateFromSearch(search)
  const inspectorSearch = demoInspectorSearch(demoState)
  const legacyDemoRequest = !demoBuild && normalizeBoolean(search.demo)
  return {
    ...(demoBuild ? {} : { demo: false }),
    ...(!demoBuild && (normalizeBoolean(search.uiDemo) || legacyDemoRequest)
      ? { uiDemo: true as const }
      : {}),
    ...(demoBuild ? inspectorSearch : {}),
  }
}

export function appVariantFromSearch(search: AppSearch): AppVariant {
  return search.demo ? 'demo' : 'live'
}

function normalizeBoolean(value: unknown) {
  if (value === true || value === '' || value === 'true' || value === 1 || value === '1') {
    return true
  }
  return false
}
