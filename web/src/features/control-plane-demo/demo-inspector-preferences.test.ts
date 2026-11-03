import { describe, expect, it } from 'vitest'
import {
  canUseDemoInspectorPlacement,
  defaultDemoInspectorPlacement,
  readDemoInspectorPlacement,
  resolveDemoInspectorPlacement,
  writeDemoInspectorPlacement,
} from './demo-inspector-preferences'

const desktop = { width: 1700, height: 900 }
const medium = { width: 1440, height: 900 }
const narrow = { width: 1024, height: 900 }
const short = { width: 1024, height: 700 }

describe('demo inspector placement preferences', () => {
  it('only enables side placements when the product has enough width', () => {
    expect(canUseDemoInspectorPlacement('left', desktop)).toBe(true)
    expect(canUseDemoInspectorPlacement('right', desktop)).toBe(true)
    expect(canUseDemoInspectorPlacement('left', medium)).toBe(false)
    expect(canUseDemoInspectorPlacement('right', medium)).toBe(false)
    expect(canUseDemoInspectorPlacement('left', narrow)).toBe(false)
    expect(canUseDemoInspectorPlacement('right', narrow)).toBe(false)
  })

  it('only enables the bottom placement when the viewport has enough height', () => {
    expect(canUseDemoInspectorPlacement('bottom', narrow)).toBe(true)
    expect(canUseDemoInspectorPlacement('bottom', short)).toBe(false)
  })

  it('falls back through bottom to floating as space becomes constrained', () => {
    expect(resolveDemoInspectorPlacement('right', narrow)).toBe('bottom')
    expect(resolveDemoInspectorPlacement('left', short)).toBe('floating')
    expect(resolveDemoInspectorPlacement('bottom', short)).toBe('floating')
  })

  it('round-trips a valid preference without changing the URL state', () => {
    let value: string | null = null
    const storage = {
      getItem: () => value,
      setItem: (_key: string, nextValue: string) => {
        value = nextValue
      },
    } as unknown as Storage

    expect(readDemoInspectorPlacement(storage)).toBe(defaultDemoInspectorPlacement)
    writeDemoInspectorPlacement('left', storage)
    expect(readDemoInspectorPlacement(storage)).toBe('left')
  })
})
