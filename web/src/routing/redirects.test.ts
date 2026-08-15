import { isRedirect } from '@tanstack/react-router'
import { describe, expect, it } from 'vitest'
import { redirectFromDeviceIndex } from './redirects'

describe('canonical route redirects', () => {
  it('replaces a collection index with the connection entry and preserves search', () => {
    try {
      redirectFromDeviceIndex({ demo: false })
      throw new Error('expected redirect')
    } catch (error) {
      expect(isRedirect(error)).toBe(true)
      if (!isRedirect(error)) return
      expect(error.options).toMatchObject({
        to: '/devices/new',
        search: { demo: false },
        replace: true,
      })
    }
  })
})
