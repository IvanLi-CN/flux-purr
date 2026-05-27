import { useEffect, useState } from 'react'

export type AppVariant = 'demo' | 'live'

const DEMO_PARAM = 'demo'
const DEMO_STORAGE_KEY = 'flux-purr.demoMode'
const DEFAULT_APP_VARIANT: AppVariant = 'demo'

export function useAppVariant() {
  const [variant] = useState(resolveInitialAppVariant)

  useEffect(() => {
    persistAppVariant(variant)
    ensureVariantUrlParam(variant)
  }, [variant])

  return variant
}

export function resolveAppVariantFromUrl(search: string, storedVariant: string | null): AppVariant {
  const params = new URLSearchParams(search)
  return (
    normalizeDemoParam(params.get(DEMO_PARAM)) ??
    normalizeStoredVariant(storedVariant) ??
    DEFAULT_APP_VARIANT
  )
}

function resolveInitialAppVariant() {
  if (typeof window === 'undefined') {
    return DEFAULT_APP_VARIANT
  }

  const fromUrl = normalizeDemoParam(new URLSearchParams(window.location.search).get(DEMO_PARAM))
  if (fromUrl) {
    persistAppVariant(fromUrl)
    return fromUrl
  }

  return (
    normalizeStoredVariant(window.localStorage.getItem(DEMO_STORAGE_KEY)) ?? DEFAULT_APP_VARIANT
  )
}

function persistAppVariant(variant: AppVariant) {
  if (typeof window === 'undefined') {
    return
  }

  window.localStorage.setItem(DEMO_STORAGE_KEY, variant === 'demo' ? 'true' : 'false')
}

function ensureVariantUrlParam(variant: AppVariant) {
  if (typeof window === 'undefined') {
    return
  }

  const url = new URL(window.location.href)
  const demoValue = variant === 'demo' ? 'true' : 'false'
  if (url.searchParams.get(DEMO_PARAM) === demoValue) {
    return
  }

  url.searchParams.delete('variant')
  url.searchParams.set(DEMO_PARAM, demoValue)
  window.history.replaceState(window.history.state, '', url)
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
