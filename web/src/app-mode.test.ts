import { describe, expect, it } from 'vitest'
import { resolveAppVariantFromUrl } from './app-mode'

describe('app variant routing', () => {
  it('lets an explicit URL variant override browser memory', () => {
    expect(resolveAppVariantFromUrl('?variant=live', 'demo')).toBe('live')
    expect(resolveAppVariantFromUrl('?variant=demo', 'live')).toBe('demo')
  })

  it('keeps the remembered variant when the URL does not explicitly switch', () => {
    expect(resolveAppVariantFromUrl('', 'live')).toBe('live')
    expect(resolveAppVariantFromUrl('?foo=bar', 'demo')).toBe('demo')
  })

  it('falls back to demo when neither URL nor browser memory is explicit', () => {
    expect(resolveAppVariantFromUrl('', null)).toBe('demo')
    expect(resolveAppVariantFromUrl('?variant=unknown', 'unknown')).toBe('demo')
  })
})
