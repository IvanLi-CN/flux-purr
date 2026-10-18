import { describe, expect, it } from 'vitest'
import { validateAppSearch } from './search'

describe('app search validation', () => {
  it('binds formal builds to live and ignores demo scene flags', () => {
    expect(
      validateAppSearch({
        demo: true,
        demoScene: 'degraded',
        demoLease: 'conflict',
        demoNetwork: 'timeout',
        demoArtifact: 'blocked',
      })
    ).toEqual({ demo: false, uiDemo: true })
  })
})
