import { describe, expect, it } from 'vitest'
import type { AppVariant } from './app-mode'

describe('app variant routing', () => {
  it('keeps the runtime variants as a build-owned type', () => {
    const variants: AppVariant[] = ['live', 'demo']
    expect(variants).toEqual(['live', 'demo'])
  })
})
