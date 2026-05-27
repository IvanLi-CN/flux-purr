import { useEffect, useState } from 'react'

export type AppVariant = 'demo' | 'live'

const APP_VARIANT_PARAM = 'variant'
const APP_VARIANT_STORAGE_KEY = 'flux-purr.appVariant'
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
    normalizeAppVariant(params.get(APP_VARIANT_PARAM)) ??
    normalizeAppVariant(storedVariant) ??
    DEFAULT_APP_VARIANT
  )
}

function resolveInitialAppVariant() {
  if (typeof window === 'undefined') {
    return DEFAULT_APP_VARIANT
  }

  const fromUrl = normalizeAppVariant(
    new URLSearchParams(window.location.search).get(APP_VARIANT_PARAM)
  )
  if (fromUrl) {
    persistAppVariant(fromUrl)
    return fromUrl
  }

  return (
    normalizeAppVariant(window.localStorage.getItem(APP_VARIANT_STORAGE_KEY)) ?? DEFAULT_APP_VARIANT
  )
}

function persistAppVariant(variant: AppVariant) {
  if (typeof window === 'undefined') {
    return
  }

  window.localStorage.setItem(APP_VARIANT_STORAGE_KEY, variant)
}

function ensureVariantUrlParam(variant: AppVariant) {
  if (typeof window === 'undefined') {
    return
  }

  const url = new URL(window.location.href)
  if (url.searchParams.get(APP_VARIANT_PARAM) === variant) {
    return
  }

  url.searchParams.set(APP_VARIANT_PARAM, variant)
  window.history.replaceState(window.history.state, '', url)
}

function normalizeAppVariant(value: string | null): AppVariant | null {
  return value === 'demo' || value === 'live' ? value : null
}
