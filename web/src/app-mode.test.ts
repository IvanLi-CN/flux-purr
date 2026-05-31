import { describe, expect, it } from 'vitest'
import { resolveAppVariantFromUrl } from './app-mode'

describe('app variant routing', () => {
  it('lets an explicit demo URL flag override browser memory', () => {
    expect(resolveAppVariantFromUrl('?demo=false', 'true')).toBe('live')
    expect(resolveAppVariantFromUrl('?demo=true', 'false')).toBe('demo')
  })

  it('keeps the remembered variant when the URL does not explicitly switch', () => {
    expect(resolveAppVariantFromUrl('', 'false')).toBe('live')
    expect(resolveAppVariantFromUrl('?foo=bar', 'true')).toBe('demo')
  })

  it('falls back to demo when neither URL nor browser memory is explicit', () => {
    expect(resolveAppVariantFromUrl('', null)).toBe('demo')
    expect(resolveAppVariantFromUrl('?demo=unknown', 'unknown')).toBe('demo')
  })
})
