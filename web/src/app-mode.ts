import { isPublicDemoBuild } from '@/public-demo'

export type AppVariant = 'demo' | 'live'

const DEMO_PARAM = 'demo'
export const DEMO_STORAGE_KEY = 'flux-purr.demoMode'
const DEFAULT_APP_VARIANT: AppVariant = 'demo'

export function resolveAppVariantFromUrl(search: string, storedVariant: string | null): AppVariant {
  if (isPublicDemoBuild()) return 'demo'
  const params = new URLSearchParams(search)
  return (
    normalizeDemoParam(params.get(DEMO_PARAM)) ??
    normalizeStoredVariant(storedVariant) ??
    DEFAULT_APP_VARIANT
  )
}

export function resolveAppVariant(value: unknown, storedVariant: string | null): AppVariant {
  if (isPublicDemoBuild()) return 'demo'
  return (
    normalizeDemoParam(typeof value === 'string' ? value : null) ??
    (typeof value === 'boolean' ? (value ? 'demo' : 'live') : null) ??
    normalizeStoredVariant(storedVariant) ??
    DEFAULT_APP_VARIANT
  )
}

export function persistAppVariant(variant: AppVariant, storage: Storage | null = browserStorage()) {
  if (isPublicDemoBuild()) return
  if (!storage) {
    return
  }
  storage.setItem(DEMO_STORAGE_KEY, variant === 'demo' ? 'true' : 'false')
}

export function readStoredAppVariant(storage: Storage | null = browserStorage()) {
  return storage?.getItem(DEMO_STORAGE_KEY) ?? null
}

function normalizeDemoParam(value: string | null): AppVariant | null {
  if (value === 'true') {
    return 'demo'
  }
  if (value === 'false') {
    return 'live'
  }
  return null
}

function normalizeStoredVariant(value: string | null): AppVariant | null {
  return normalizeDemoParam(value) ?? (value === 'demo' || value === 'live' ? value : null)
}

function browserStorage() {
  return typeof window === 'undefined' ? null : window.localStorage
}
