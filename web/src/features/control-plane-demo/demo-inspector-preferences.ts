export const demoInspectorPlacementIds = ['left', 'right', 'bottom', 'floating'] as const

export type DemoInspectorPlacement = (typeof demoInspectorPlacementIds)[number]

export interface DemoInspectorViewport {
  width: number
  height: number
}

export const defaultDemoInspectorPlacement: DemoInspectorPlacement = 'right'

const demoInspectorPlacementStorageKey = 'flux-purr:demo-inspector-placement'
const sidePlacementMinWidth = 1700
const bottomPlacementMinHeight = 760

export function isDemoInspectorPlacement(value: unknown): value is DemoInspectorPlacement {
  return (
    typeof value === 'string' && demoInspectorPlacementIds.includes(value as DemoInspectorPlacement)
  )
}

export function canUseDemoInspectorPlacement(
  placement: DemoInspectorPlacement,
  viewport: DemoInspectorViewport
) {
  if (placement === 'left' || placement === 'right') return viewport.width >= sidePlacementMinWidth
  if (placement === 'bottom') return viewport.height >= bottomPlacementMinHeight
  return true
}

export function resolveDemoInspectorPlacement(
  preferred: DemoInspectorPlacement,
  viewport: DemoInspectorViewport
): DemoInspectorPlacement {
  if (canUseDemoInspectorPlacement(preferred, viewport)) return preferred
  if (canUseDemoInspectorPlacement('bottom', viewport)) return 'bottom'
  return 'floating'
}

export function readDemoInspectorPlacement(storage?: Storage | null): DemoInspectorPlacement {
  try {
    const value = storage?.getItem(demoInspectorPlacementStorageKey)
    return isDemoInspectorPlacement(value) ? value : defaultDemoInspectorPlacement
  } catch {
    return defaultDemoInspectorPlacement
  }
}

export function writeDemoInspectorPlacement(
  placement: DemoInspectorPlacement,
  storage?: Storage | null
) {
  try {
    storage?.setItem(demoInspectorPlacementStorageKey, placement)
  } catch {
    // Storage can be unavailable in embedded previews; the in-memory preference still applies.
  }
}
