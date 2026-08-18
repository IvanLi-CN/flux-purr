import { describe, expect, it, vi } from 'vitest'

import {
  browserSecurityBlockMessage,
  connectBrowserLoader,
  disconnectBrowserLoader,
  ESP_GET_SECURITY_INFO,
  getEsp32S3SecurityInfo,
  parseEsp32S3SecurityInfo,
  preflightBrowserLayout,
  preflightBrowserLoader,
  writeBrowserBundle,
} from './browser-esptool'
import type { ValidatedFirmwareBundle } from './types'

const esptoolMocks = vi.hoisted(() => ({
  detectChip: vi.fn(),
  main: vi.fn(),
  requestPort: vi.fn(),
}))

vi.mock('esptool-js', () => {
  class Transport {
    disconnect = vi.fn().mockResolvedValue(undefined)
  }

  class ESPLoader {
    chip = {}
    detectChip = esptoolMocks.detectChip
    main = esptoolMocks.main
  }

  return { ESPLoader, Transport }
})

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

  it('normalizes the four-byte ROM response trailer before parsing security info', async () => {
    const payload = new Uint8Array(24)
    const command = async (op: number) => {
      expect(op).toBe(ESP_GET_SECURITY_INFO)
      return [0, payload] as [number, Uint8Array]
    }

    await expect(getEsp32S3SecurityInfo({ command } as never)).resolves.toEqual({
      secureBootEnabled: false,
      flashEncryptionEnabled: false,
      secureDownloadModeEnabled: false,
      responseKnown: true,
    })
  })

  it('accepts an extended ROM response after stripping its transport trailer', async () => {
    const payload = new Uint8Array(28)
    payload[0] = 0x05
    payload[4] = 0x07
    payload.set([0xde, 0xad, 0xbe, 0xef], 24)
    const command = async (op: number) => {
      expect(op).toBe(ESP_GET_SECURITY_INFO)
      return [0, payload] as [number, Uint8Array]
    }

    await expect(getEsp32S3SecurityInfo({ command } as never)).resolves.toEqual({
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

  it('fails closed for an incomplete ROM security response', async () => {
    const command = async () => [0, new Uint8Array(23)] as [number, Uint8Array]

    await expect(getEsp32S3SecurityInfo({ command } as never)).resolves.toMatchObject({
      responseKnown: false,
    })
  })

  it('makes the security block reason actionable without permitting a write', () => {
    expect(
      browserSecurityBlockMessage({
        secureBootEnabled: true,
        flashEncryptionEnabled: false,
        secureDownloadModeEnabled: true,
        responseKnown: true,
      })
    ).toBe('芯片安全状态阻止浏览器烧录：Secure Boot 已启用、Secure Download Mode 已启用。')
  })

  it('releases a failed loader before another preflight may open the port', async () => {
    const disconnect = vi.fn().mockResolvedValue(undefined)
    await disconnectBrowserLoader({ transport: { disconnect } } as never)
    expect(disconnect).toHaveBeenCalledOnce()
  })

  it('releases the ROM port when security preflight blocks the target', async () => {
    const disconnect = vi.fn().mockResolvedValue(undefined)
    const payload = new Uint8Array(20)
    payload[0] = 0x01
    const loader = {
      chip: {
        CHIP_NAME: 'ESP32-S3',
        getChipFeatures: async () => ['Embedded Flash 4MB', 'Embedded PSRAM 2MB'],
      },
      command: async () => [0, payload] as [number, Uint8Array],
      detectFlashSize: async () => '4MB',
      transport: { disconnect },
    }

    await expect(
      preflightBrowserLoader(loader as never, {} as ValidatedFirmwareBundle, 'install_recovery')
    ).rejects.toThrow('Secure Boot 已启用')
    expect(disconnect).toHaveBeenCalledOnce()
  })
})

describe('Browser ROM connection', () => {
  it('keeps the loader in ROM mode for the security probe', async () => {
    const windowDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'window')
    const navigatorDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'navigator')
    const port = {} as never
    esptoolMocks.detectChip.mockReset().mockResolvedValue(undefined)
    esptoolMocks.main.mockReset().mockResolvedValue(undefined)
    esptoolMocks.requestPort.mockReset().mockResolvedValue(port)
    Object.defineProperty(globalThis, 'window', {
      configurable: true,
      value: { isSecureContext: true },
    })
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: { serial: { requestPort: esptoolMocks.requestPort } },
    })

    try {
      await connectBrowserLoader()
      expect(esptoolMocks.detectChip).toHaveBeenCalledWith('default_reset')
      expect(esptoolMocks.main).not.toHaveBeenCalled()
    } finally {
      if (windowDescriptor) Object.defineProperty(globalThis, 'window', windowDescriptor)
      else Reflect.deleteProperty(globalThis, 'window')
      if (navigatorDescriptor) Object.defineProperty(globalThis, 'navigator', navigatorDescriptor)
      else Reflect.deleteProperty(globalThis, 'navigator')
    }
  })

  it('reads security from ROM before uploading the flasher stub', async () => {
    const calls: string[] = []
    const loader = {
      chip: {
        CHIP_NAME: 'ESP32-S3',
        getChipFeatures: async () => {
          calls.push('features')
          return ['Embedded Flash 4MB', 'Embedded PSRAM 2MB']
        },
      },
      command: async (op: number) => {
        expect(op).toBe(ESP_GET_SECURITY_INFO)
        calls.push('security')
        return [0, new Uint8Array(20)] as [number, Uint8Array]
      },
      detectFlashSize: async () => {
        calls.push('flash-size')
        return '4MB'
      },
      runStub: async () => {
        calls.push('stub')
      },
      transport: { disconnect: vi.fn().mockResolvedValue(undefined) },
    }

    await expect(
      preflightBrowserLoader(loader as never, {} as ValidatedFirmwareBundle, 'install_recovery')
    ).resolves.toEqual({ sourcePartitionTableSha256: null, configCopy: null })
    expect(calls).toEqual(['flash-size', 'features', 'security', 'stub'])
  })

  it('does not permit a browser write without a successful ROM preflight', async () => {
    await expect(
      writeBrowserBundle({} as never, {} as ValidatedFirmwareBundle, 'install_recovery')
    ).rejects.toThrow('Browser ROM preflight must complete for this operation before writing.')
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
