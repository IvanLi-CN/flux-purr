import { type AppVariant, readStoredAppVariant, resolveAppVariant } from '@/app-mode'
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
  demo: boolean
  uiDemo?: true
  demoScene?: DemoSceneId
  demoLease?: DemoLeaseState
  demoNetwork?: DemoNetworkState
  demoArtifact?: DemoArtifactState
}

export function validateAppSearch(search: Record<string, unknown>): AppSearch {
  const variant = resolveAppVariant(search.demo, readStoredAppVariant())
  const demoState = demoInspectorStateFromSearch(search)
  const inspectorSearch = demoInspectorSearch(demoState)
  return {
    demo: variant === 'demo',
    ...(!isPublicDemoBuild() && normalizeBoolean(search.uiDemo) ? { uiDemo: true as const } : {}),
    ...(variant === 'demo' ? inspectorSearch : {}),
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
