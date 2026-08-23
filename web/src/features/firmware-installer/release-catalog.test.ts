import { afterEach, describe, expect, it, vi } from 'vitest'

const RC_SHA256 = `sha256:${'a'.repeat(64)}`

vi.mock('./bundle', () => ({
  validateFirmwareBundle: vi.fn(async (bytes: Uint8Array) => ({
    manifest: {
      identity: {
        channel: 'rc',
        version: '1.5.0-rc.1',
        sourceSha: 'a'.repeat(40),
        buildId: 'a'.repeat(16),
      },
    },
    bundleSha256: `sha256:${'a'.repeat(64)}`,
    archiveSize: bytes.byteLength,
  })),
}))

import { fetchOfficialBundle, fetchOfficialCatalog } from './release-catalog'

afterEach(() => vi.unstubAllGlobals())

describe('same-origin firmware catalog', () => {
  it('lists exact same-origin release paths and downloads only the selected bundle', async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            schemaVersion: 1,
            generatedAt: '2026-08-16T08:00:00Z',
            releaseCount: 2,
            releases: [
              {
                id: 'release:v1.4.2:1',
                version: '1.4.2',
                channel: 'stable',
                source: 'release',
                releaseTag: 'v1.4.2',
                sourceSha: 'b'.repeat(40),
                buildId: 'b'.repeat(16),
                publishedAt: '2026-07-20T08:00:00Z',
                bundleSha256: `sha256:${'b'.repeat(64)}`,
                size: 42,
                target: 'ESP32-S3FH4R2',
                assetPath: 'firmware/releases/v1.4.2-1/flux-purr-v1.4.2.fluxpurr-fw',
              },
              {
                id: 'release:v1.5.0-rc.1:2',
                version: '1.5.0-rc.1',
                channel: 'rc',
                source: 'release',
                releaseTag: 'v1.5.0-rc.1',
                sourceSha: 'a'.repeat(40),
                buildId: 'a'.repeat(16),
                publishedAt: '2026-08-08T08:00:00Z',
                bundleSha256: RC_SHA256,
                size: 42,
                target: 'ESP32-S3FH4R2',
                assetPath: 'firmware/releases/v1.5.0-rc.1-2/flux-purr-v1.5.0-rc.1.fluxpurr-fw',
              },
            ],
          })
        )
      )
      .mockResolvedValueOnce(new Response(new Uint8Array([1, 2, 3])))
    vi.stubGlobal('fetch', fetch)

    const artifacts = await fetchOfficialCatalog()
    const rcArtifact = artifacts.find((artifact) => artifact.channel === 'rc')
    if (!rcArtifact) throw new Error('RC artifact fixture is missing.')

    expect(artifacts.map((artifact) => artifact.version)).toEqual(['1.5.0-rc.1', '1.4.2'])
    expect(fetch.mock.calls[0][0]).toBe('/firmware/releases-manifest.json')
    expect(fetch.mock.calls[0][1]).toMatchObject({
      headers: { Accept: 'application/json' },
    })

    const result = await fetchOfficialBundle(rcArtifact)

    expect(result.bytes).toEqual(new Uint8Array([1, 2, 3]))
    expect(fetch.mock.calls[1][0]).toBe(
      '/firmware/releases/v1.5.0-rc.1-2/flux-purr-v1.5.0-rc.1.fluxpurr-fw'
    )
    expect(String(fetch.mock.calls[0][0])).not.toContain('github')
    expect(String(fetch.mock.calls[1][0])).not.toContain('github')
  })

  it('rejects a catalog that attempts to point the browser at an external asset', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            schemaVersion: 1,
            generatedAt: '2026-08-16T08:00:00Z',
            releaseCount: 1,
            releases: [
              {
                id: 'release:unsafe',
                version: '1.5.0',
                channel: 'stable',
                source: 'release',
                releaseTag: 'unsafe',
                sourceSha: 'a'.repeat(40),
                buildId: 'a'.repeat(16),
                publishedAt: '2026-08-08T08:00:00Z',
                bundleSha256: RC_SHA256,
                size: 42,
                target: 'ESP32-S3FH4R2',
                assetPath: 'https://api.github.com/repos/IvanLi-CN/flux-purr/releases/assets/1',
              },
            ],
          })
        )
      )
    )

    await expect(fetchOfficialCatalog()).rejects.toThrow('Same-origin firmware catalog is invalid')
  })
})
