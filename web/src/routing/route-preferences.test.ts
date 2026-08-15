import { describe, expect, it } from 'vitest'
import {
  ROUTE_PREFERENCES_STORAGE_KEY,
  readRoutePreferences,
  rememberSuccessfulRoute,
} from './route-preferences'

class MemoryStorage {
  readonly values = new Map<string, string>()
  getItem(key: string) {
    return this.values.get(key) ?? null
  }
  setItem(key: string, value: string) {
    this.values.set(key, value)
  }
}

describe('route preferences', () => {
  it('stores only variant identities and transport kinds', () => {
    const storage = new MemoryStorage()
    rememberSuccessfulRoute('demo', 'device-1', 'mock', storage)
    rememberSuccessfulRoute('live', 'device-2', 'wifi', storage)

    expect(readRoutePreferences(storage)).toEqual({
      lastDeviceByVariant: { demo: 'device-1', live: 'device-2' },
      transportByIdentity: { 'device-1': 'mock', 'device-2': 'wifi' },
    })
    expect(storage.getItem(ROUTE_PREFERENCES_STORAGE_KEY)).not.toContain('password')
  })

  it('ignores malformed and unsupported values', () => {
    const storage = new MemoryStorage()
    storage.setItem(
      ROUTE_PREFERENCES_STORAGE_KEY,
      JSON.stringify({
        lastDeviceByVariant: { demo: 42, live: 'device-live' },
        transportByIdentity: { 'device-live': 'bluetooth', valid: 'bridge' },
      })
    )
    expect(readRoutePreferences(storage)).toEqual({
      lastDeviceByVariant: { live: 'device-live' },
      transportByIdentity: { valid: 'bridge' },
    })

    storage.setItem(ROUTE_PREFERENCES_STORAGE_KEY, '{broken')
    expect(readRoutePreferences(storage)).toEqual({
      lastDeviceByVariant: {},
      transportByIdentity: {},
    })
  })
})
