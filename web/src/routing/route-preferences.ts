import type { AppVariant } from '@/app-mode'
import type { DeviceConnectionKind } from '@/features/control-plane-demo/device-target-picker'

export const ROUTE_PREFERENCES_STORAGE_KEY = 'flux-purr.routePreferences.v1'

export interface RoutePreferences {
  lastDeviceByVariant: Partial<Record<AppVariant, string>>
  transportByIdentity: Record<string, DeviceConnectionKind>
}

interface StorageLike {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
}

const emptyPreferences = (): RoutePreferences => ({
  lastDeviceByVariant: {},
  transportByIdentity: {},
})

export function readRoutePreferences(storage: StorageLike | null = browserStorage()) {
  if (!storage) return emptyPreferences()
  try {
    return normalizePreferences(JSON.parse(storage.getItem(ROUTE_PREFERENCES_STORAGE_KEY) ?? '{}'))
  } catch {
    return emptyPreferences()
  }
}

export function rememberSuccessfulRoute(
  variant: AppVariant,
  identityId: string,
  transport: DeviceConnectionKind,
  storage: StorageLike | null = browserStorage()
) {
  if (!storage || !identityId.trim()) return
  const current = readRoutePreferences(storage)
  storage.setItem(
    ROUTE_PREFERENCES_STORAGE_KEY,
    JSON.stringify({
      lastDeviceByVariant: { ...current.lastDeviceByVariant, [variant]: identityId },
      transportByIdentity: { ...current.transportByIdentity, [identityId]: transport },
    } satisfies RoutePreferences)
  )
}

function normalizePreferences(value: unknown): RoutePreferences {
  if (!value || typeof value !== 'object') return emptyPreferences()
  const record = value as Record<string, unknown>
  const lastDeviceByVariant: RoutePreferences['lastDeviceByVariant'] = {}
  const rawLast = record.lastDeviceByVariant
  if (rawLast && typeof rawLast === 'object') {
    for (const variant of ['demo', 'live'] as const) {
      const identity = (rawLast as Record<string, unknown>)[variant]
      if (typeof identity === 'string' && identity.trim()) lastDeviceByVariant[variant] = identity
    }
  }
  const transportByIdentity: RoutePreferences['transportByIdentity'] = {}
  const rawTransports = record.transportByIdentity
  if (rawTransports && typeof rawTransports === 'object') {
    for (const [identity, transport] of Object.entries(rawTransports)) {
      if (
        identity.trim() &&
        (transport === 'wifi' ||
          transport === 'web-serial' ||
          transport === 'bridge' ||
          transport === 'mock')
      ) {
        transportByIdentity[identity] = transport
      }
    }
  }
  return { lastDeviceByVariant, transportByIdentity }
}

function browserStorage(): StorageLike | null {
  return typeof window === 'undefined' ? null : window.localStorage
}
