import { describe, expect, it, vi } from 'vitest'
import type { BrowserWriteProgressEvent } from './browser-esptool'
import {
  browserSecurityBlockMessage,
  connectBrowserLoader,
  disconnectBrowserLoader,
  ESP_GET_SECURITY_INFO,
  getEsp32S3SecurityInfo,
  parseEsp32S3SecurityInfo,
  preflightBrowserLayout,
  preflightBrowserLoader,
  verifyBrowserRuntime,
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
      expect(esptoolMocks.requestPort).toHaveBeenCalledWith({
        filters: [{ usbVendorId: 0x303a, usbProductId: 0x1001 }],
      })
      expect(esptoolMocks.detectChip).toHaveBeenCalledWith('usb_reset')
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

  it('uses the ESP32-S3 USB-JTAG stub handoff from the reference implementation', async () => {
    const checkCommand = vi.fn().mockResolvedValue(undefined)
    const loader = {
      chip: {
        CHIP_NAME: 'ESP32-S3',
        getChipFeatures: async () => ['Embedded Flash 4MB', 'Embedded PSRAM 2MB'],
      },
      command: async () => [0, new Uint8Array(20)] as [number, Uint8Array],
      detectFlashSize: async () => '4MB',
      ESP_MEM_END: 0x06,
      _intToByteArray: (value: number) => new Uint8Array([value, 0, 0, 0]),
      _appendArray: (left: Uint8Array, right: Uint8Array) => new Uint8Array([...left, ...right]),
      checkCommand,
      runStub: async () => {
        await (loader as unknown as { memFinish(entrypoint: number): Promise<void> }).memFinish(0)
      },
      transport: { disconnect: vi.fn().mockResolvedValue(undefined) },
    }

    await preflightBrowserLoader(loader as never, {} as ValidatedFirmwareBundle, 'install_recovery')

    expect(checkCommand).toHaveBeenCalledWith(
      'leave RAM download mode',
      0x06,
      new Uint8Array([1, 0, 0, 0, 0, 0, 0, 0]),
      undefined,
      undefined,
      2_000
    )
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

describe('Browser write progress', () => {
  function fixtureBundle() {
    const segments = [
      {
        kind: 'bootloader',
        path: 'bootloader.bin',
        address: 0,
        length: 2,
        sha256: 'sha256:bootloader',
        md5: 'bootloader-md5',
      },
      {
        kind: 'partition-table',
        path: 'partition-table.bin',
        address: 0x8000,
        length: 3,
        sha256: 'sha256:partition-table',
        md5: 'partition-table-md5',
      },
      {
        kind: 'factory-app',
        path: 'factory-app.bin',
        address: 0x10000,
        length: 5,
        sha256: 'sha256:factory-app',
        md5: 'factory-app-md5',
      },
    ] as const
    return {
      manifest: {
        segments,
      },
      images: new Map([
        ['bootloader.bin', new Uint8Array(2)],
        ['partition-table.bin', new Uint8Array(3)],
        ['factory-app.bin', new Uint8Array(5)],
      ]),
    } as unknown as ValidatedFirmwareBundle
  }

  function preparedLoader(overrides: Record<string, unknown> = {}) {
    const bundle = fixtureBundle()
    const loader = {
      chip: {
        CHIP_NAME: 'ESP32-S3',
        getChipFeatures: async () => ['Embedded Flash 4MB', 'Embedded PSRAM 2MB'],
      },
      command: async () => [0, new Uint8Array(20)] as [number, Uint8Array],
      detectFlashSize: async () => '4MB',
      runStub: vi.fn().mockResolvedValue(undefined),
      transport: { disconnect: vi.fn().mockResolvedValue(undefined) },
      eraseFlash: vi.fn().mockResolvedValue(undefined),
      writeFlash: vi.fn(async ({ reportProgress }) => {
        reportProgress?.(0, 1, 2)
        reportProgress?.(2, 5, 5)
      }),
      readFlash: vi.fn(),
      flashMd5sum: vi.fn(async (address: number) => {
        const segment = bundle.manifest.segments.find((candidate) => candidate.address === address)
        return segment?.md5 ?? ''
      }),
      after: vi.fn().mockResolvedValue(undefined),
      ...overrides,
    }
    return { bundle, loader }
  }

  it('reports operation boundaries only after they complete and keeps byte/segment units', async () => {
    const { bundle, loader } = preparedLoader()
    const events: BrowserWriteProgressEvent[] = []
    const legacyProgress = vi.fn()
    await preflightBrowserLoader(loader as never, bundle, 'install_recovery')

    await writeBrowserBundle(loader as never, bundle, 'install_recovery', {
      reportProgress: legacyProgress,
      reportStage: (event) => events.push(event),
    })

    expect(events).toEqual([
      { stage: 'erase_started' },
      { stage: 'erase_completed' },
      { stage: 'write_started', totalBytes: 10 },
      { stage: 'write_progress', segmentIndex: 0, written: 1, total: 2 },
      { stage: 'write_progress', segmentIndex: 2, written: 5, total: 5 },
      { stage: 'rom_md5_started', totalSegments: 3 },
      { stage: 'rom_md5_progress', segmentIndex: 0, completedSegments: 1, totalSegments: 3 },
      { stage: 'rom_md5_progress', segmentIndex: 1, completedSegments: 2, totalSegments: 3 },
      { stage: 'rom_md5_progress', segmentIndex: 2, completedSegments: 3, totalSegments: 3 },
      { stage: 'reset_started' },
      { stage: 'reset_completed' },
    ])
    expect(legacyProgress.mock.calls).toEqual([
      [0, 1, 2],
      [2, 5, 5],
    ])
    expect(loader.after.mock.calls).toEqual([
      ['hard_reset'],
      ['custom_reset', undefined, 'D0|R0|W50|D1|R0|W50|D0|R1|W50|D0|R0|W250'],
    ])
  })

  it('does not report completion for a hardware boundary that rejects', async () => {
    const { bundle, loader } = preparedLoader({
      eraseFlash: vi.fn().mockRejectedValue(new Error('erase rejected')),
    })
    const stages: string[] = []
    await preflightBrowserLoader(loader as never, bundle, 'install_recovery')

    await expect(
      writeBrowserBundle(loader as never, bundle, 'install_recovery', {
        reportStage: ({ stage }) => stages.push(stage),
      })
    ).rejects.toThrow('erase rejected')
    expect(stages).toEqual(['erase_started'])
  })
})

describe('Browser runtime verification', () => {
  const bundle = {
    manifest: {
      identity: {
        version: '0.18.3-dev.c682ef3',
        sourceSha: 'c682ef3-source',
        buildId: 'browser-runtime-test',
      },
      layout: {
        id: 'esp32-s3fh4r2-v1',
        version: 1,
        partitionTableSha256: 'sha256:partition-table',
      },
    },
  } as unknown as ValidatedFirmwareBundle

  function runtimeFrames(prefix = '') {
    return new TextEncoder().encode(
      `${prefix}${JSON.stringify({
        type: 'response',
        requestId: 'firmware-identity',
        ok: true,
        result: {
          identity: {
            firmwareVersion: bundle.manifest.identity.version,
            gitSha: bundle.manifest.identity.sourceSha,
            buildId: bundle.manifest.identity.buildId,
          },
        },
      })}\n${JSON.stringify({
        type: 'response',
        requestId: 'firmware-install-status',
        ok: true,
        result: {
          installStatus: {
            layoutId: bundle.manifest.layout.id,
            layoutVersion: bundle.manifest.layout.version,
            partitionTableSha256: bundle.manifest.layout.partitionTableSha256,
          },
        },
      })}\n`
    )
  }

  it('retries only the selected port while USB CDC returns after reset', async () => {
    const stages: string[] = []
    const port = {
      readable: null as ReadableStream<Uint8Array> | null,
      writable: null as WritableStream<Uint8Array> | null,
      open: vi.fn(async () => {
        if (port.open.mock.calls.length === 1)
          throw new DOMException('Device unavailable', 'NetworkError')
        if (port.open.mock.calls.length === 2)
          throw new Error('Failed to open serial port: device busy')
        port.readable = new ReadableStream({
          start(controller) {
            controller.enqueue(runtimeFrames())
          },
        })
        port.writable = new WritableStream()
      }),
      close: vi.fn().mockResolvedValue(undefined),
    }
    const loader = {
      transport: {
        device: port,
        disconnect: vi.fn().mockResolvedValue(undefined),
      },
    }

    await expect(
      verifyBrowserRuntime(loader as never, bundle, {
        timeoutMs: 250,
        reconnectDelayMs: 0,
        reconnectRetryMs: 1,
        boundaryTimeoutMs: 50,
        reportStage: ({ stage }) => stages.push(stage),
      })
    ).resolves.toMatchObject({
      identity: { firmwareVersion: bundle.manifest.identity.version },
      installStatus: { layoutId: bundle.manifest.layout.id },
    })
    expect(port.open).toHaveBeenCalledTimes(3)
    expect(stages).toEqual(
      expect.arrayContaining([
        'disconnecting_rom',
        'waiting_for_runtime',
        'opening_runtime',
        'requesting_identity',
        'reading_runtime',
        'closing_runtime',
      ])
    )
  })

  it('does not disconnect the ROM transport twice after the caller released it', async () => {
    const writes: string[] = []
    const port = {
      readable: null as ReadableStream<Uint8Array> | null,
      writable: null as WritableStream<Uint8Array> | null,
      open: vi.fn(async () => {
        port.readable = new ReadableStream({
          start(controller) {
            controller.enqueue(runtimeFrames())
          },
        })
        port.writable = new WritableStream({
          write(chunk: Uint8Array) {
            writes.push(JSON.parse(new TextDecoder().decode(chunk)).op)
          },
        })
      }),
      close: vi.fn().mockResolvedValue(undefined),
    }
    const disconnect = vi.fn().mockResolvedValue(undefined)
    const loader = { transport: { device: port, disconnect } }

    await verifyBrowserRuntime(loader as never, bundle, {
      timeoutMs: 500,
      reconnectDelayMs: 0,
      requestRetryMs: 1,
      romTransportAlreadyDisconnected: true,
    })

    expect(disconnect).not.toHaveBeenCalled()
    expect(writes).toContain('get_identity')
  })

  it('re-resolves the unique same USB target after native USB re-enumeration', async () => {
    const navigatorDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'navigator')
    const stalePort = {
      readable: null,
      writable: null,
      getInfo: () => ({ usbVendorId: 0x303a, usbProductId: 0x1001 }),
      open: vi.fn(),
      close: vi.fn().mockResolvedValue(undefined),
    }
    const writes: string[] = []
    const refreshedPort = {
      readable: null as ReadableStream<Uint8Array> | null,
      writable: null as WritableStream<Uint8Array> | null,
      getInfo: () => ({ usbVendorId: 0x303a, usbProductId: 0x1001 }),
      open: vi.fn(async () => {
        refreshedPort.readable = new ReadableStream({
          start(controller) {
            controller.enqueue(runtimeFrames())
          },
        })
        refreshedPort.writable = new WritableStream({
          write(chunk: Uint8Array) {
            writes.push(JSON.parse(new TextDecoder().decode(chunk)).op)
          },
        })
      }),
      close: vi.fn().mockResolvedValue(undefined),
    }
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: { serial: { getPorts: vi.fn().mockResolvedValue([refreshedPort]) } },
    })

    try {
      await expect(
        verifyBrowserRuntime(
          {
            transport: { device: stalePort, disconnect: vi.fn().mockResolvedValue(undefined) },
          } as never,
          bundle,
          { timeoutMs: 500, reconnectDelayMs: 0, requestRetryMs: 1 }
        )
      ).resolves.toMatchObject({ identity: { buildId: bundle.manifest.identity.buildId } })
      expect(stalePort.open).not.toHaveBeenCalled()
      expect(refreshedPort.open).toHaveBeenCalledOnce()
      expect(writes).toContain('get_identity')
    } finally {
      if (navigatorDescriptor) Object.defineProperty(globalThis, 'navigator', navigatorDescriptor)
      else Reflect.deleteProperty(globalThis, 'navigator')
    }
  })

  it('fails closed when re-enumeration loses the selected port USB identity', async () => {
    const navigatorDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'navigator')
    const stalePort = {
      readable: null,
      writable: null,
      open: vi.fn(),
      close: vi.fn().mockResolvedValue(undefined),
    }
    const unrelatedPort = {
      readable: null,
      writable: null,
      getInfo: () => ({ usbVendorId: 0x303a, usbProductId: 0x1001 }),
      open: vi.fn(),
      close: vi.fn().mockResolvedValue(undefined),
    }
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: { serial: { getPorts: vi.fn().mockResolvedValue([unrelatedPort]) } },
    })

    try {
      await expect(
        verifyBrowserRuntime(
          {
            transport: { device: stalePort, disconnect: vi.fn().mockResolvedValue(undefined) },
          } as never,
          bundle,
          { timeoutMs: 500, reconnectDelayMs: 0 }
        )
      ).rejects.toThrow('cannot prove the selected Web USB target after reset')
      expect(unrelatedPort.open).not.toHaveBeenCalled()
    } finally {
      if (navigatorDescriptor) Object.defineProperty(globalThis, 'navigator', navigatorDescriptor)
      else Reflect.deleteProperty(globalThis, 'navigator')
    }
  })

  it('refuses ambiguous same-model ports after reset instead of choosing one', async () => {
    const navigatorDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'navigator')
    const stalePort = {
      readable: null,
      writable: null,
      getInfo: () => ({ usbVendorId: 0x303a, usbProductId: 0x1001 }),
      open: vi.fn(),
      close: vi.fn().mockResolvedValue(undefined),
    }
    const matchingPort = () => ({
      readable: null,
      writable: null,
      getInfo: () => ({ usbVendorId: 0x303a, usbProductId: 0x1001 }),
      open: vi.fn(),
      close: vi.fn().mockResolvedValue(undefined),
    })
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: { serial: { getPorts: vi.fn().mockResolvedValue([matchingPort(), matchingPort()]) } },
    })

    try {
      await expect(
        verifyBrowserRuntime(
          {
            transport: { device: stalePort, disconnect: vi.fn().mockResolvedValue(undefined) },
          } as never,
          bundle,
          { timeoutMs: 500, reconnectDelayMs: 0 }
        )
      ).rejects.toThrow('ports are ambiguous after reset')
      expect(stalePort.open).not.toHaveBeenCalled()
    } finally {
      if (navigatorDescriptor) Object.defineProperty(globalThis, 'navigator', navigatorDescriptor)
      else Reflect.deleteProperty(globalThis, 'navigator')
    }
  })

  it('settles with an opening-stage error when SerialPort.open never resolves', async () => {
    const port = {
      readable: null,
      writable: null,
      open: vi.fn(() => new Promise<void>(() => undefined)),
      close: vi.fn().mockResolvedValue(undefined),
    }
    const loader = {
      transport: {
        device: port,
        disconnect: vi.fn().mockResolvedValue(undefined),
      },
    }

    await expect(
      verifyBrowserRuntime(loader as never, bundle, {
        timeoutMs: 40,
        reconnectDelayMs: 0,
        boundaryTimeoutMs: 10,
      })
    ).rejects.toThrow('opening runtime serial port')
  })

  it('retries identity requests while firmware initializes and ignores boot text', async () => {
    let readController: ReadableStreamDefaultController<Uint8Array> | null = null
    let writeCount = 0
    const port = {
      readable: null as ReadableStream<Uint8Array> | null,
      writable: null as WritableStream<Uint8Array> | null,
      open: vi.fn(async () => {
        port.readable = new ReadableStream({
          start(controller) {
            readController = controller
          },
        })
        port.writable = new WritableStream({
          write() {
            writeCount += 1
            if (writeCount === 2) readController?.enqueue(runtimeFrames('ESP-ROM boot trace\n'))
          },
        })
      }),
      close: vi.fn().mockResolvedValue(undefined),
    }
    const loader = {
      transport: {
        device: port,
        disconnect: vi.fn().mockResolvedValue(undefined),
      },
    }

    await expect(
      verifyBrowserRuntime(loader as never, bundle, {
        timeoutMs: 1_000,
        reconnectDelayMs: 0,
        requestRetryMs: 2,
      })
    ).resolves.toMatchObject({ identity: { buildId: bundle.manifest.identity.buildId } })
    expect(writeCount).toBe(2)
  })

  it('waits for runtime_ready before retrying a startup-busy install-status request', async () => {
    let readController: ReadableStreamDefaultController<Uint8Array> | null = null
    const writes: string[] = []
    const encoder = new TextEncoder()
    const port = {
      readable: null as ReadableStream<Uint8Array> | null,
      writable: null as WritableStream<Uint8Array> | null,
      open: vi.fn(async () => {
        port.readable = new ReadableStream({
          start(controller) {
            readController = controller
          },
        })
        port.writable = new WritableStream({
          write(chunk: Uint8Array) {
            const request = new TextDecoder().decode(chunk)
            writes.push(request)
            if (writes.length === 1) {
              readController?.enqueue(
                encoder.encode(
                  `${JSON.stringify({
                    type: 'response',
                    requestId: 'firmware-identity',
                    ok: true,
                    result: {
                      identity: {
                        firmwareVersion: bundle.manifest.identity.version,
                        gitSha: bundle.manifest.identity.sourceSha,
                        buildId: bundle.manifest.identity.buildId,
                      },
                    },
                  })}\n`
                )
              )
            }
            if (writes.length === 2) {
              readController?.enqueue(
                encoder.encode(
                  `${JSON.stringify({
                    type: 'error',
                    requestId: 'firmware-install-status',
                    error: { code: 'startup_busy' },
                  })}\n`
                )
              )
              setTimeout(() => {
                readController?.enqueue(encoder.encode('boot_stage=runtime_ready\n'))
              }, 5)
            }
            if (writes.length === 3) {
              readController?.enqueue(
                encoder.encode(
                  `${JSON.stringify({
                    type: 'response',
                    requestId: 'firmware-install-status',
                    ok: true,
                    result: {
                      installStatus: {
                        layoutId: bundle.manifest.layout.id,
                        layoutVersion: bundle.manifest.layout.version,
                        partitionTableSha256: bundle.manifest.layout.partitionTableSha256,
                      },
                    },
                  })}\n`
                )
              )
            }
          },
        })
      }),
      close: vi.fn().mockResolvedValue(undefined),
    }
    const loader = {
      transport: {
        device: port,
        disconnect: vi.fn().mockResolvedValue(undefined),
      },
    }

    await expect(
      verifyBrowserRuntime(loader as never, bundle, {
        timeoutMs: 500,
        reconnectDelayMs: 0,
        requestRetryMs: 1,
      })
    ).resolves.toMatchObject({ installStatus: { layoutId: bundle.manifest.layout.id } })
    expect(writes).toHaveLength(3)
    expect(writes.map((write) => JSON.parse(write).op)).toEqual([
      'get_identity',
      'get_install_status',
      'get_install_status',
    ])
  })

  it('keeps querying the selected port when the best-effort boot marker is absent', async () => {
    let readController: ReadableStreamDefaultController<Uint8Array> | null = null
    const writes: string[] = []
    const encoder = new TextEncoder()
    const port = {
      readable: null as ReadableStream<Uint8Array> | null,
      writable: null as WritableStream<Uint8Array> | null,
      open: vi.fn(async () => {
        port.readable = new ReadableStream({
          start(controller) {
            readController = controller
          },
        })
        port.writable = new WritableStream({
          write(chunk: Uint8Array) {
            writes.push(new TextDecoder().decode(chunk))
            if (writes.length === 4) {
              readController?.enqueue(
                encoder.encode(
                  `${JSON.stringify({
                    type: 'response',
                    requestId: 'firmware-identity',
                    ok: true,
                    result: {
                      identity: {
                        firmwareVersion: bundle.manifest.identity.version,
                        gitSha: bundle.manifest.identity.sourceSha,
                        buildId: bundle.manifest.identity.buildId,
                      },
                    },
                  })}\n`
                )
              )
            }
            if (writes.length === 5) {
              readController?.enqueue(
                encoder.encode(
                  `${JSON.stringify({
                    type: 'response',
                    requestId: 'firmware-install-status',
                    ok: true,
                    result: {
                      installStatus: {
                        layoutId: bundle.manifest.layout.id,
                        layoutVersion: bundle.manifest.layout.version,
                        partitionTableSha256: bundle.manifest.layout.partitionTableSha256,
                      },
                    },
                  })}\n`
                )
              )
            }
          },
        })
      }),
      close: vi.fn().mockResolvedValue(undefined),
    }
    const loader = {
      transport: {
        device: port,
        disconnect: vi.fn().mockResolvedValue(undefined),
      },
    }

    await expect(
      verifyBrowserRuntime(loader as never, bundle, {
        timeoutMs: 500,
        reconnectDelayMs: 0,
        requestRetryMs: 1,
      })
    ).resolves.toMatchObject({ identity: { buildId: bundle.manifest.identity.buildId } })
    expect(writes.map((write) => JSON.parse(write).op)).toEqual([
      'get_identity',
      'get_identity',
      'get_identity',
      'get_identity',
      'get_install_status',
    ])
  })

  it('retries identity once after runtime_ready arrives after both early requests', async () => {
    let readController: ReadableStreamDefaultController<Uint8Array> | null = null
    const writes: string[] = []
    const encoder = new TextEncoder()
    const port = {
      readable: null as ReadableStream<Uint8Array> | null,
      writable: null as WritableStream<Uint8Array> | null,
      open: vi.fn(async () => {
        port.readable = new ReadableStream({
          start(controller) {
            readController = controller
          },
        })
        port.writable = new WritableStream({
          write(chunk: Uint8Array) {
            writes.push(new TextDecoder().decode(chunk))
            if (writes.length === 2) {
              readController?.enqueue(encoder.encode('boot_stage=runtime_ready\n'))
            }
            if (writes.length === 3) {
              readController?.enqueue(
                encoder.encode(
                  `${JSON.stringify({
                    type: 'response',
                    requestId: 'firmware-identity',
                    ok: true,
                    result: {
                      identity: {
                        firmwareVersion: bundle.manifest.identity.version,
                        gitSha: bundle.manifest.identity.sourceSha,
                        buildId: bundle.manifest.identity.buildId,
                      },
                    },
                  })}\n`
                )
              )
            }
            if (writes.length === 4) {
              readController?.enqueue(
                encoder.encode(
                  `${JSON.stringify({
                    type: 'response',
                    requestId: 'firmware-install-status',
                    ok: true,
                    result: {
                      installStatus: {
                        layoutId: bundle.manifest.layout.id,
                        layoutVersion: bundle.manifest.layout.version,
                        partitionTableSha256: bundle.manifest.layout.partitionTableSha256,
                      },
                    },
                  })}\n`
                )
              )
            }
          },
        })
      }),
      close: vi.fn().mockResolvedValue(undefined),
    }
    const loader = {
      transport: {
        device: port,
        disconnect: vi.fn().mockResolvedValue(undefined),
      },
    }

    await expect(
      verifyBrowserRuntime(loader as never, bundle, {
        timeoutMs: 500,
        reconnectDelayMs: 0,
        requestRetryMs: 1,
      })
    ).resolves.toMatchObject({ identity: { buildId: bundle.manifest.identity.buildId } })
    expect(writes.map((write) => JSON.parse(write).op)).toEqual([
      'get_identity',
      'get_identity',
      'get_identity',
      'get_install_status',
    ])
  })

  it('cancels a pending read and releases stream locks before closing', async () => {
    const cancel = vi.fn()
    const port = {
      readable: null as ReadableStream<Uint8Array> | null,
      writable: null as WritableStream<Uint8Array> | null,
      open: vi.fn(async () => {
        port.readable = new ReadableStream({ cancel })
        port.writable = new WritableStream()
      }),
      close: vi.fn(async () => {
        expect(port.readable?.locked).toBe(false)
        expect(port.writable?.locked).toBe(false)
      }),
    }
    const loader = {
      transport: {
        device: port,
        disconnect: vi.fn().mockResolvedValue(undefined),
      },
    }

    await expect(
      verifyBrowserRuntime(loader as never, bundle, {
        timeoutMs: 40,
        reconnectDelayMs: 0,
        boundaryTimeoutMs: 10,
        requestRetryMs: 5,
      })
    ).rejects.toThrow('Runtime verification timed out')
    expect(cancel).toHaveBeenCalledOnce()
    expect(port.close).toHaveBeenCalledOnce()
  })
})
