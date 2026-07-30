import { describe, expect, it, vi } from 'vitest'
import {
  claimLanPairing,
  isChromiumPrivateNetworkSupported,
  lanProbeToDeviceTarget,
  listSavedLanDeviceSessions,
  normalizeLanBaseUrl,
  probeLanDevice,
} from './lan-client'

describe('LAN browser client', () => {
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
})
