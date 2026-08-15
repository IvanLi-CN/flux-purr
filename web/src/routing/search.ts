import { type AppVariant, readStoredAppVariant, resolveAppVariant } from '@/app-mode'

export interface AppSearch {
  demo: boolean
  uiDemo?: true
}

export function validateAppSearch(search: Record<string, unknown>): AppSearch {
  const variant = resolveAppVariant(search.demo, readStoredAppVariant())
  return {
    demo: variant === 'demo',
    ...(normalizeBoolean(search.uiDemo) ? { uiDemo: true as const } : {}),
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
