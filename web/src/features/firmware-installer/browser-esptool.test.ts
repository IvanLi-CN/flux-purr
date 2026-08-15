import { describe, expect, it } from 'vitest'

import {
  ESP_GET_SECURITY_INFO,
  getEsp32S3SecurityInfo,
  parseEsp32S3SecurityInfo,
  preflightBrowserLayout,
} from './browser-esptool'
import type { ValidatedFirmwareBundle } from './types'

describe('ESP32-S3 GET_SECURITY_INFO adapter', () => {
  it('uses only ROM command 0x14 and decodes fail-closed fields', async () => {
    const payload = new Uint8Array(20)
    payload[0] = 0x05
    payload[4] = 0x07
    const command = async (op: number) => {
      expect(op).toBe(ESP_GET_SECURITY_INFO)
      return [0, payload] as [number, Uint8Array]
    }
    const info = await getEsp32S3SecurityInfo({ command } as never)
    expect(info).toEqual({
      secureBootEnabled: true,
      flashEncryptionEnabled: true,
      secureDownloadModeEnabled: true,
      responseKnown: true,
    })
  })

  it('treats short or unknown responses as unknown', () => {
    expect(parseEsp32S3SecurityInfo(new Uint8Array(4)).responseKnown).toBe(false)
    const unknownFlags = new Uint8Array(20)
    unknownFlags[1] = 0x80
    expect(parseEsp32S3SecurityInfo(unknownFlags).responseKnown).toBe(false)
  })
})

describe('browser layout and configuration preflight', () => {
  const bundle = (partitionTableSha256: string, migrationIds: string[] = []) =>
    ({
      manifest: {
        layout: { partitionTableSha256 },
        migrations: migrationIds,
      },
    }) as ValidatedFirmwareBundle

  it('accepts only an exact current partition-table hash for same-layout update', async () => {
    const partitionTable = new Uint8Array(0x1000).fill(0x5a)
    const digest = await crypto.subtle.digest('SHA-256', partitionTable)
    const hash = `sha256:${Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('')}`
    const result = await preflightBrowserLayout(
      { readFlash: async () => partitionTable } as never,
      bundle(hash),
      'update'
    )
    expect(result).toEqual({ sourcePartitionTableSha256: hash, configCopy: null })
  })

  it('blocks an unknown source layout', async () => {
    await expect(
      preflightBrowserLayout(
        { readFlash: async () => new Uint8Array(0x1000) } as never,
        bundle('sha256:not-the-source'),
        'update'
      )
    ).rejects.toThrow('no declared supported migration')
  })

  it('does not inspect a source layout for install or recovery', async () => {
    const readFlash = () => Promise.reject(new Error('must not read'))
    await expect(
      preflightBrowserLayout({ readFlash } as never, bundle('sha256:target'), 'install_recovery')
    ).resolves.toEqual({ sourcePartitionTableSha256: null, configCopy: null })
  })
})
