import { afterEach, describe, expect, it, vi } from 'vitest'

vi.mock('./bundle', () => ({
  validateFirmwareBundle: vi.fn(async (bytes: Uint8Array) => ({
    manifest: { identity: { channel: 'rc' } },
    archiveSize: bytes.byteLength,
  })),
}))

import { fetchOfficialBundle } from './release-catalog'

afterEach(() => vi.unstubAllGlobals())

describe('official firmware catalog', () => {
  it('selects only a prerelease bundle for the RC channel', async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            { prerelease: false, assets: [] },
            {
              prerelease: true,
              assets: [{ name: 'flux-purr.fluxpurr-fw', browser_download_url: 'https://asset' }],
            },
          ])
        )
      )
      .mockResolvedValueOnce(new Response(new Uint8Array([1, 2, 3])))
    vi.stubGlobal('fetch', fetch)
    const result = await fetchOfficialBundle('rc')
    expect(result.bytes).toEqual(new Uint8Array([1, 2, 3]))
    expect(fetch.mock.calls[0][0]).toContain('releases?per_page=20')
  })
})
