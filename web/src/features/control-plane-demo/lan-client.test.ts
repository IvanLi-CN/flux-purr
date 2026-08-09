import { describe, expect, it, vi } from 'vitest'
import {
  authorizedLanRequest,
  claimLanPairing,
  getLanPublicInfo,
  isChromiumPrivateNetworkSupported,
  lanProbeToDeviceTarget,
  listSavedLanDeviceSessions,
  loadLanAddress,
  loadLanDeviceSession,
  loadLanScanCidr,
  normalizeLanBaseUrl,
  probeLanDevice,
  rememberLanDeviceIdentity,
  resumeLanDeviceSession,
  savedLanSessionToDeviceTarget,
  scanLanSubnet,
  storeLanAddress,
  storeLanDeviceSession,
  storeLanScanCidr,
  streamLanEvents,
  upsertLanDeviceTarget,
  writeLanRuntime,
} from './lan-client'
import type { DeviceTarget } from './types'

describe('LAN browser client', () => {
  it('retains the last explicit device address and removes it when cleared', () => {
    const records = new Map<string, string>()
    const storage = {
      get length() {
        return records.size
      },
      clear: () => records.clear(),
      getItem: (key: string) => records.get(key) ?? null,
      key: () => null,
      removeItem: (key: string) => records.delete(key),
      setItem: (key: string, value: string) => records.set(key, value),
    } as Storage
    vi.stubGlobal('window', { localStorage: storage })
    try {
      expect(loadLanAddress('http://192.168.1.18')).toBe('http://192.168.1.18')

      storeLanAddress(' http://192.168.1.42 ')
      expect(loadLanAddress('http://192.168.1.18')).toBe('http://192.168.1.42')

      storeLanAddress('')
      expect(loadLanAddress('http://192.168.1.18')).toBe('http://192.168.1.18')
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('retains the last explicit direct-LAN CIDR without keeping a blank draft', () => {
    const records = new Map<string, string>()
    const storage = {
      get length() {
        return records.size
      },
      clear: () => records.clear(),
      getItem: (key: string) => records.get(key) ?? null,
      key: () => null,
      removeItem: (key: string) => records.delete(key),
      setItem: (key: string, value: string) => records.set(key, value),
    } as Storage
    vi.stubGlobal('window', { localStorage: storage })
    try {
      expect(loadLanScanCidr('192.168.1.0/24')).toBe('192.168.1.0/24')

      storeLanScanCidr('192.168.31.0/24')
      expect(loadLanScanCidr('192.168.1.0/24')).toBe('192.168.31.0/24')

      storeLanScanCidr('')
      expect(loadLanScanCidr('192.168.1.0/24')).toBe('192.168.1.0/24')
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('scans the selected private IPv4 /24 through anonymous browser health requests only', async () => {
    const requestedUrls: string[] = []
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      requestedUrls.push(url)
      expect(url).not.toContain('/api/v1/lan/discovery')
      expect(url.endsWith('/health')).toBe(true)
      expect(init?.headers).not.toHaveProperty('Authorization')
      expect((init as RequestInit & { targetAddressSpace?: string }).targetAddressSpace).toBe(
        'private'
      )
      if (url === 'http://192.168.1.42/health') {
        return new Response(
          JSON.stringify({
            ok: true,
            api: 'v1',
            deviceId: '001122334455',
            hostname: 'flux-purr-001122334455',
            firmwareVersion: '1.0.0',
            pairing: { mode: 'required', active: false, attemptsRemaining: 5 },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } }
        )
      }
      throw new TypeError('host unavailable')
    }) as unknown as typeof fetch

    await expect(
      scanLanSubnet('192.168.1.0/24', { fetcher, concurrency: 8, timeoutMs: 50 })
    ).resolves.toEqual([
      {
        baseUrl: 'http://192.168.1.42',
        info: {
          api: 'v1',
          deviceId: '001122334455',
          hostname: 'flux-purr-001122334455',
          firmwareVersion: '1.0.0',
          pairing: { mode: 'required', active: false, attemptsRemaining: 5 },
        },
      },
    ])
    expect(requestedUrls).toHaveLength(254)
  })

  it('reports browser scan progress and stops issuing requests after cancellation', async () => {
    const controller = new AbortController()
    const progress: Array<{ done: number; total: number }> = []
    const fetcher = vi.fn(async () => {
      controller.abort()
      throw new TypeError('host unavailable')
    }) as unknown as typeof fetch

    await scanLanSubnet('192.168.1.0/24', {
      fetcher,
      concurrency: 1,
      timeoutMs: 50,
      signal: controller.signal,
      onProgress: (next) => progress.push(next),
    })

    expect(fetcher).toHaveBeenCalledTimes(1)
    expect(progress).toEqual([{ done: 1, total: 254 }])
  })

  it('rejects public or oversized CIDR ranges before any browser request', async () => {
    const fetcher = vi.fn() as unknown as typeof fetch

    await expect(scanLanSubnet('8.8.8.0/24', { fetcher })).rejects.toMatchObject({
      code: 'lan_scan_cidr_public',
    })
    await expect(scanLanSubnet('192.168.0.0/16', { fetcher })).rejects.toMatchObject({
      code: 'lan_scan_cidr_too_large',
    })
    expect(fetcher).not.toHaveBeenCalled()
  })

  it('reads public device information before it asks for a pairing code', async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      expect(String(input)).toBe('http://192.168.1.10/health')
      expect(init?.headers).not.toHaveProperty('Authorization')
      expect(init?.body).toBeUndefined()
      return new Response(
        JSON.stringify({
          ok: true,
          api: 'v1',
          deviceId: '001122334455',
          hostname: 'flux-purr-001122334455',
          firmwareVersion: '1.0.0',
          pairing: { mode: 'required', active: true, attemptsRemaining: 5 },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } }
      )
    }) as unknown as typeof fetch

    await expect(getLanPublicInfo('http://192.168.1.10', fetcher)).resolves.toMatchObject({
      deviceId: '001122334455',
      pairing: { mode: 'required', active: true },
    })
    expect(fetcher).toHaveBeenCalledTimes(1)
  })

  it('resumes the saved browser session only after health confirms the same device identity', async () => {
    const records = new Map<string, string>()
    const storage = {
      get length() {
        return records.size
      },
      clear: () => records.clear(),
      getItem: (key: string) => records.get(key) ?? null,
      key: (index: number) => Array.from(records.keys())[index] ?? null,
      removeItem: (key: string) => records.delete(key),
      setItem: (key: string, value: string) => records.set(key, value),
    } as Storage
    vi.stubGlobal('window', { localStorage: storage })
    try {
      const session = {
        baseUrl: 'http://192.168.1.10',
        token: 'a'.repeat(64),
        deviceId: '001122334455',
        hostname: 'flux-purr-001122334455',
      }
      storeLanDeviceSession(session)
      const probe = {
        identity: {
          deviceId: '001122334455',
          hostname: 'flux-purr-001122334455',
          firmwareVersion: '1.0.0',
          buildId: 'build-1',
          capabilities: [],
        },
      } as unknown as Awaited<ReturnType<typeof probeLanDevice>>

      await expect(
        resumeLanDeviceSession(session.baseUrl, { deviceId: '001122334455' }, async () => probe)
      ).resolves.toMatchObject({ session, probe })

      await expect(
        resumeLanDeviceSession(session.baseUrl, { deviceId: 'aabbccddeeff' }, async () => probe)
      ).resolves.toBeNull()
      expect(loadLanDeviceSession(session.baseUrl)).toBeNull()
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('accepts Chromium but explicitly excludes Safari direct-LAN mode', () => {
    expect(isChromiumPrivateNetworkSupported('Mozilla/5.0 Chrome/140.0.0.0 Safari/537.36')).toBe(
      true
    )
    expect(isChromiumPrivateNetworkSupported('Mozilla/5.0 Version/18.0 Safari/605.1.15')).toBe(
      false
    )
  })

  it('uses PNA fetch options and never carries token in URL', async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      expect(String(input)).not.toContain('aaaaaaaa')
      expect(init?.headers).toMatchObject({ Authorization: `Bearer ${'a'.repeat(64)}` })
      expect((init as RequestInit & { targetAddressSpace?: string }).targetAddressSpace).toBe(
        'private'
      )
      return new Response(JSON.stringify({}), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      })
    }) as unknown as typeof fetch
    await probeLanDevice({ baseUrl: 'http://192.168.1.10', token: 'a'.repeat(64) }, fetcher)
    expect(fetcher).toHaveBeenCalledTimes(3)
  })

  it('limits the initial LAN probe to the two device HTTP workers and remembers the newest revision', async () => {
    let activeRequests = 0
    let maximumActiveRequests = 0
    const requestOrder: string[] = []
    const records = new Map<string, string>()
    vi.stubGlobal('window', {
      localStorage: {
        get length() {
          return records.size
        },
        clear: () => records.clear(),
        getItem: (key: string) => records.get(key) ?? null,
        key: (index: number) => Array.from(records.keys())[index] ?? null,
        removeItem: (key: string) => records.delete(key),
        setItem: (key: string, value: string) => records.set(key, value),
      },
    })
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      activeRequests += 1
      maximumActiveRequests = Math.max(maximumActiveRequests, activeRequests)
      const path = new URL(String(input)).pathname
      requestOrder.push(path)
      await new Promise((resolve) => setTimeout(resolve, 1))
      activeRequests -= 1
      return new Response(JSON.stringify({}), {
        status: 200,
        headers: { 'content-type': 'application/json', 'X-Flux-Purr-Revision': '7' },
      })
    }) as unknown as typeof fetch
    const session: import('./lan-client').LanDeviceSession = {
      baseUrl: 'http://192.168.1.10',
      token: 'a'.repeat(64),
    }

    try {
      await expect(probeLanDevice(session, fetcher)).resolves.toBeDefined()
      expect(requestOrder).toEqual(
        expect.arrayContaining(['/api/v1/identity', '/api/v1/network', '/api/v1/status'])
      )
      expect(maximumActiveRequests).toBe(2)
      expect(session.controlRevision).toBe(7)
      expect(JSON.parse(records.get('flux-purr:lan-device:http://192.168.1.10') ?? '{}')).toEqual(
        expect.objectContaining({ controlRevision: 7 })
      )
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('keeps the first probe after pairing serial until the claim socket is released', async () => {
    let activeRequests = 0
    let maximumActiveRequests = 0
    const requestOrder: string[] = []
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      activeRequests += 1
      maximumActiveRequests = Math.max(maximumActiveRequests, activeRequests)
      requestOrder.push(new URL(String(input)).pathname)
      await new Promise((resolve) => setTimeout(resolve, 1))
      activeRequests -= 1
      return new Response(JSON.stringify({}), {
        status: 200,
        headers: { 'content-type': 'application/json', 'X-Flux-Purr-Revision': '7' },
      })
    }) as unknown as typeof fetch

    await probeLanDevice(
      { baseUrl: 'http://192.168.1.10', token: 'a'.repeat(64) },
      fetcher,
      'serial'
    )

    expect(maximumActiveRequests).toBe(1)
    expect(requestOrder).toEqual(['/api/v1/identity', '/api/v1/network', '/api/v1/status'])
  })

  it('replaces a persisted revision with the serial preflight revision after a device reboot', async () => {
    const records = new Map<string, string>()
    vi.stubGlobal('window', {
      localStorage: {
        get length() {
          return records.size
        },
        clear: () => records.clear(),
        getItem: (key: string) => records.get(key) ?? null,
        key: (index: number) => Array.from(records.keys())[index] ?? null,
        removeItem: (key: string) => records.delete(key),
        setItem: (key: string, value: string) => records.set(key, value),
      },
    })
    const session: import('./lan-client').LanDeviceSession = {
      baseUrl: 'http://192.168.1.10',
      token: 'a'.repeat(64),
      controlRevision: 42,
    }
    const fetcher = vi.fn(async () => {
      return new Response(JSON.stringify({}), {
        status: 200,
        headers: { 'content-type': 'application/json', 'X-Flux-Purr-Revision': '7' },
      })
    }) as unknown as typeof fetch

    try {
      await probeLanDevice(session, fetcher, 'serial')
      expect(session.controlRevision).toBe(7)
      expect(JSON.parse(records.get('flux-purr:lan-device:http://192.168.1.10') ?? '{}')).toEqual(
        expect.objectContaining({ controlRevision: 7 })
      )
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('falls back to a serial probe when the two-worker read batch hits a transport limit', async () => {
    let activeRequests = 0
    let concurrentFailure = false
    const fetcher = vi.fn(async (_input: RequestInfo | URL) => {
      activeRequests += 1
      try {
        if (activeRequests > 1 && !concurrentFailure) {
          concurrentFailure = true
          throw new TypeError('Failed to fetch')
        }
        await new Promise((resolve) => setTimeout(resolve, 1))
        return new Response(JSON.stringify({}), {
          status: 200,
          headers: { 'content-type': 'application/json', 'X-Flux-Purr-Revision': '7' },
        })
      } finally {
        activeRequests -= 1
      }
    }) as unknown as typeof fetch

    await expect(
      probeLanDevice({ baseUrl: 'http://192.168.1.10', token: 'a'.repeat(64) }, fetcher)
    ).resolves.toBeDefined()
    expect(concurrentFailure).toBe(true)
    expect(fetcher).toHaveBeenCalledTimes(5)
  })

  it('refreshes the device revision immediately before every LAN write', async () => {
    const requestPaths: string[] = []
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input)).pathname
      requestPaths.push(path)
      if (path === '/api/v1/status') {
        return new Response(JSON.stringify({}), {
          status: 200,
          headers: { 'content-type': 'application/json', 'X-Flux-Purr-Revision': '11' },
        })
      }
      expect(init?.headers).toMatchObject({
        'X-Flux-Purr-Lease': 'lease-1',
        'X-Flux-Purr-Revision': '11',
      })
      return new Response(JSON.stringify({ accepted: true }), {
        status: 200,
        headers: { 'content-type': 'application/json', 'X-Flux-Purr-Revision': '8' },
      })
    }) as unknown as typeof fetch
    const session = {
      baseUrl: 'http://192.168.1.10',
      token: 'a'.repeat(64),
      controlRevision: 7,
    }

    await writeLanRuntime(session, 'lease-1', { targetTempC: 120 }, fetcher)

    expect(requestPaths).toEqual(['/api/v1/status', '/api/v1/runtime'])
    expect(session.controlRevision).toBe(8)
  })

  it('uses the same fresh-revision preflight for non-runtime control writes', async () => {
    const requests: Array<{ path: string; headers?: HeadersInit }> = []
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input)).pathname
      requests.push({ path, headers: init?.headers })
      const revision = path === '/api/v1/status' ? '15' : '16'
      return new Response(JSON.stringify({}), {
        status: 200,
        headers: { 'content-type': 'application/json', 'X-Flux-Purr-Revision': revision },
      })
    }) as unknown as typeof fetch
    const session = {
      baseUrl: 'http://192.168.1.10',
      token: 'a'.repeat(64),
      controlRevision: 3,
    }

    await authorizedLanRequest(session, 'calibration', 'PUT', { op: 'clear' }, 'lease-1', fetcher)

    expect(requests.map((request) => request.path)).toEqual([
      '/api/v1/status',
      '/api/v1/calibration',
    ])
    expect(requests[1]?.headers).toMatchObject({ 'X-Flux-Purr-Revision': '15' })
    expect(session.controlRevision).toBe(16)
  })

  it('persists a successful LAN write revision for the next reloaded write', async () => {
    const records = new Map<string, string>()
    vi.stubGlobal('window', {
      localStorage: {
        get length() {
          return records.size
        },
        clear: () => records.clear(),
        getItem: (key: string) => records.get(key) ?? null,
        key: (index: number) => Array.from(records.keys())[index] ?? null,
        removeItem: (key: string) => records.delete(key),
        setItem: (key: string, value: string) => records.set(key, value),
      },
    })
    const revisions: string[] = []
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input)).pathname
      if (path === '/api/v1/status') {
        const current = revisions.length === 0 ? '7' : '8'
        return new Response(JSON.stringify({}), {
          status: 200,
          headers: { 'content-type': 'application/json', 'X-Flux-Purr-Revision': current },
        })
      }
      const revision = (init?.headers as Record<string, string>)['X-Flux-Purr-Revision']
      revisions.push(revision)
      return new Response(JSON.stringify({ accepted: true }), {
        status: 200,
        headers: {
          'content-type': 'application/json',
          'X-Flux-Purr-Revision': String(Number(revision) + 1),
        },
      })
    }) as unknown as typeof fetch
    const initialSession = {
      baseUrl: 'http://192.168.1.10',
      token: 'a'.repeat(64),
      controlRevision: 7,
    }

    try {
      storeLanDeviceSession(initialSession)
      await writeLanRuntime(initialSession, 'lease-1', { targetTempC: 120 }, fetcher)

      const reloadedSession = loadLanDeviceSession(initialSession.baseUrl)
      expect(reloadedSession?.controlRevision).toBe(8)
      if (!reloadedSession) {
        throw new Error('expected persisted LAN session')
      }
      await writeLanRuntime(reloadedSession, 'lease-1', { targetTempC: 121 }, fetcher)

      expect(revisions).toEqual(['7', '8'])
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('keeps a remembered LAN connection visible while the device is offline', () => {
    const records = new Map<string, string>()
    vi.stubGlobal('window', {
      localStorage: {
        get length() {
          return records.size
        },
        clear: () => records.clear(),
        getItem: (key: string) => records.get(key) ?? null,
        key: (index: number) => Array.from(records.keys())[index] ?? null,
        removeItem: (key: string) => records.delete(key),
        setItem: (key: string, value: string) => records.set(key, value),
      },
    })
    const session = { baseUrl: 'http://192.168.31.189', token: 'a'.repeat(64) }
    const probe = {
      identity: {
        deviceId: 'a0f262f20d6c',
        hostname: 'flux-purr-a0f262f20d6c',
        firmwareVersion: '0.1.0',
        buildId: 'build-1',
        gitSha: 'sha',
        board: 'esp32s3',
        apiVersion: 'v1',
        protocolVersion: 'usb.v1',
        capabilities: [],
      },
      network: { state: 'connected' as const },
      status: {} as import('./contracts').ControlPlaneStatus,
    }

    try {
      const remembered = rememberLanDeviceIdentity(session, probe)
      const reloaded = listSavedLanDeviceSessions()[0]
      const target = savedLanSessionToDeviceTarget(reloaded)

      expect(reloaded).toEqual(remembered)
      expect(target).toMatchObject({
        id: 'lan-a0f262f20d6c',
        identityId: 'a0f262f20d6c',
        alias: 'flux-purr-a0f262f20d6c',
        transport: 'wifi',
        severity: 'offline',
        baseUrl: 'http://192.168.31.189',
      })
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('restores a legacy LAN session from its canonical hostname', () => {
    expect(
      savedLanSessionToDeviceTarget({
        baseUrl: 'http://192.168.31.189',
        token: 'a'.repeat(64),
        hostname: 'flux-purr-a0f262f20d6c',
      })
    ).toMatchObject({
      id: 'lan-a0f262f20d6c',
      identityId: 'a0f262f20d6c',
      alias: 'flux-purr-a0f262f20d6c',
      transport: 'wifi',
      severity: 'offline',
    })
  })

  it('validates manual addresses and pairing codes before issuing a request', async () => {
    expect(normalizeLanBaseUrl('http://192.168.1.10/')).toBe('http://192.168.1.10')
    expect(normalizeLanBaseUrl('http://flux-purr-001122334455.local/')).toBe(
      'http://flux-purr-001122334455.local'
    )
    expect(() => normalizeLanBaseUrl('https://192.168.1.10/api')).toThrow()
    expect(() => normalizeLanBaseUrl('http://8.8.8.8')).toThrow()
    expect(() => normalizeLanBaseUrl('http://example.com')).toThrow()
    await expect(
      claimLanPairing('http://192.168.1.10', '12', vi.fn() as unknown as typeof fetch)
    ).rejects.toMatchObject({ code: 'pairing_code_invalid' })
  })

  it('classifies a malformed successful pairing response as a protocol error', async () => {
    const fetcher = vi.fn(
      async () =>
        new Response('{"token":"broken","api":"v1,}', {
          status: 200,
          headers: { 'content-type': 'application/json' },
        })
    ) as unknown as typeof fetch

    await expect(claimLanPairing('http://192.168.1.10', '4827', fetcher)).rejects.toMatchObject({
      code: 'lan_response_invalid',
      message: '设备返回的数据格式无效。',
    })
  })

  it('turns a paired probe into a direct WiFi control target without carrying the token', () => {
    const target = lanProbeToDeviceTarget(
      { baseUrl: 'http://192.168.1.10', token: 'a'.repeat(64), hostname: 'flux-purr-test' },
      {
        identity: {
          deviceId: '001122334455',
          firmwareVersion: '1.0.0',
          buildId: 'build',
          gitSha: 'sha',
          board: 'esp32s3',
          apiVersion: 'v1',
          protocolVersion: 'usb.v1',
          hostname: 'flux-purr-test',
          capabilities: [],
        },
        network: { state: 'connected', ip: '192.168.1.10', wifiRssi: -48 },
        status: {
          mode: 'idle',
          uptimeSeconds: 4,
          currentTempC: 25,
          targetTempC: 120,
          heaterEnabled: false,
          heaterOutputPercent: 0,
          activeCoolingEnabled: true,
          fanDisplayState: 'AUTO',
          fanEnabled: true,
          fanPwmPermille: 400,
          voltageMv: 20000,
          currentMa: 0,
          boardTempCenti: 2500,
          pdRequestMv: 20000,
          pdContractMv: 20000,
          pdState: 'ready',
          calibration: {
            mode: 'off',
            ppsEnabled: false,
            heaterEnabled: false,
            stable: false,
            job: { status: 'idle', progressPercent: 0, samplesCollected: 0 },
          },
          network: { state: 'connected' },
        },
      }
    )
    expect(target.transport).toBe('wifi')
    expect(JSON.stringify(target)).not.toContain('a'.repeat(64))
  })

  it('updates a paired LAN target by stable device id when DHCP changes its address', () => {
    const current = {
      id: 'lan-001122334455',
      alias: 'flux-purr-001122334455',
      location: '192.168.1.10',
      transport: 'wifi',
      severity: 'nominal',
      baseUrl: 'http://192.168.1.10',
    } as DeviceTarget
    const replacement = {
      ...current,
      location: '192.168.1.42',
      baseUrl: 'http://192.168.1.42',
    }

    expect(upsertLanDeviceTarget([current], replacement)).toEqual([replacement])
  })

  it('lists only valid locally saved LAN sessions', () => {
    const records = new Map<string, string>()
    const storage = {
      get length() {
        return records.size
      },
      key: (index: number) => Array.from(records.keys())[index] ?? null,
      getItem: (key: string) => records.get(key) ?? null,
      setItem: (key: string, value: string) => {
        records.set(key, value)
      },
      removeItem: (key: string) => {
        records.delete(key)
      },
      clear: () => {
        records.clear()
      },
    } as Storage
    vi.stubGlobal('window', { localStorage: storage })
    try {
      storage.setItem(
        'flux-purr:lan-device:http://192.168.1.10',
        JSON.stringify({ baseUrl: 'http://192.168.1.10', token: 'a'.repeat(64) })
      )
      storage.setItem('flux-purr:lan-device:invalid', '{not json')

      expect(listSavedLanDeviceSessions()).toEqual([
        { baseUrl: 'http://192.168.1.10', token: 'a'.repeat(64) },
      ])
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('invalidates credentials without forgetting the device when a bearer request returns 401', async () => {
    const records = new Map<string, string>()
    const storage = {
      get length() {
        return records.size
      },
      key: (index: number) => Array.from(records.keys())[index] ?? null,
      getItem: (key: string) => records.get(key) ?? null,
      setItem: (key: string, value: string) => {
        records.set(key, value)
      },
      removeItem: (key: string) => {
        records.delete(key)
      },
      clear: () => {
        records.clear()
      },
    } as Storage
    vi.stubGlobal('window', { localStorage: storage })
    try {
      const token = 'a'.repeat(64)
      storage.setItem(
        'flux-purr:lan-device:http://192.168.1.10',
        JSON.stringify({
          baseUrl: 'http://192.168.1.10',
          token,
          deviceId: '001122334455',
          hostname: 'flux-purr-001122334455',
        })
      )
      storage.setItem(
        'flux-purr:lan-device:http://192.168.1.11',
        JSON.stringify({ baseUrl: 'http://192.168.1.11', token })
      )

      await expect(
        probeLanDevice(
          { baseUrl: 'http://192.168.1.10', token },
          vi.fn(
            async () =>
              new Response(JSON.stringify({ error: { code: 'unauthorized' } }), {
                status: 401,
                headers: { 'content-type': 'application/json' },
              })
          ) as unknown as typeof fetch
        )
      ).rejects.toMatchObject({ code: 'unauthorized' })

      expect(loadLanDeviceSession('http://192.168.1.10')).toBeNull()
      expect(loadLanDeviceSession('http://192.168.1.11')).toMatchObject({ token })
      expect(listSavedLanDeviceSessions()).toContainEqual(
        expect.objectContaining({
          baseUrl: 'http://192.168.1.10',
          deviceId: '001122334455',
          authorizationState: 'invalid',
        })
      )
      const invalidSession = listSavedLanDeviceSessions().find(
        (session) => session.baseUrl === 'http://192.168.1.10'
      )
      expect(invalidSession).toBeDefined()
      expect(invalidSession && savedLanSessionToDeviceTarget(invalidSession)).toMatchObject({
        id: 'lan-001122334455',
        severity: 'offline',
      })
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('reconnects the LAN event stream after a transient reader failure', async () => {
    vi.useFakeTimers()
    try {
      let attempts = 0
      const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        attempts += 1
        expect(String(input)).not.toContain('aaaaaaaa')
        expect(init?.headers).toMatchObject({ Authorization: `Bearer ${'a'.repeat(64)}` })
        expect((init as RequestInit & { targetAddressSpace?: string }).targetAddressSpace).toBe(
          'private'
        )
        if (attempts === 1) {
          return new Response(
            new ReadableStream<Uint8Array>({
              start(controller) {
                controller.error(new TypeError('temporary WiFi disconnect'))
              },
            }),
            { status: 200 }
          )
        }
        return new Response('data: {"targetTempC":145}\n\n', { status: 200 })
      }) as unknown as typeof fetch
      const controller = new AbortController()
      const events = streamLanEvents(
        { baseUrl: 'http://192.168.1.10', token: 'a'.repeat(64) },
        controller.signal,
        fetcher
      )
      const next = events.next()

      await vi.advanceTimersByTimeAsync(1_000)
      await expect(next).resolves.toMatchObject({ done: false, value: { targetTempC: 145 } })
      expect(fetcher).toHaveBeenCalledTimes(2)

      controller.abort()
      await events.return(undefined)
    } finally {
      vi.useRealTimers()
    }
  })
})
