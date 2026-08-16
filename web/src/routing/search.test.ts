import { describe, expect, it } from 'vitest'
import { validateAppSearch } from './search'

describe('app search validation', () => {
  it('keeps only canonical Demo Inspector query values', () => {
    expect(
      validateAppSearch({
        demo: true,
        demoScene: 'degraded',
        demoLease: 'conflict',
        demoNetwork: 'timeout',
        demoArtifact: 'blocked',
      })
    ).toMatchObject({
      demo: true,
      demoScene: 'degraded',
      demoLease: 'conflict',
      demoNetwork: 'timeout',
      demoArtifact: 'blocked',
    })
  })
})
