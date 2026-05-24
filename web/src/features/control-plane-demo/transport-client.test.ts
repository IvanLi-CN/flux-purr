import { describe, expect, it, vi } from 'vitest'
import type { DevdDeviceRecord } from './contracts'
import {
  artifactToManifest,
  createControlPlaneHttpClient,
  createUsbWifiConfigFrame,
  devdRecordToDeviceTarget,
  manifestToArtifact,
  redactWifiConfigFrame,
} from './transport-client'

describe('control-plane transport client', () => {
  it('maps devd records into demo device targets', () => {
    const record: DevdDeviceRecord = {
      id: 'mock-fp-lab-01',
      displayName: 'Bench Alpha',
      portPath: null,
      transport: 'native_serial',
      connection: 'connected',
      identity: {
        deviceId: 'mock-fp-lab-01',
        firmwareVersion: 'fw/v0.4.0-dev',
        buildId: 'devd-mock',
        gitSha: 'abc',
        board: 'esp32-s3',
        apiVersion: '2026-05-23',
        protocolVersion: 'flux-purr.usb.v1',
        hostname: 'mock-fp-lab-01',
        capabilities: ['identity', 'status', 'wifi_config'],
      },
      network: {
        state: 'connected',
        ssid: 'FluxPurr-Lab',
        wifiRssi: -54,
      },
      status: {
        mode: 'sampling',
        uptimeSeconds: 3661,
        currentTempC: 183.6,
        targetTempC: 220,
        heaterEnabled: true,
        heaterOutputPercent: 22,
        activeCoolingEnabled: true,
        fanDisplayState: 'AUTO',
        fanEnabled: true,
        fanPwmPermille: 500,
        voltageMv: 20010,
        currentMa: 840,
        boardTempCenti: 3840,
        pdRequestMv: 20000,
        pdContractMv: 20000,
        pdState: 'ready',
        frontpanelKey: null,
        network: {
          state: 'connected',
          wifiRssi: -54,
        },
      },
    }

    const target = devdRecordToDeviceTarget(record)

    expect(target.transport).toBe('devd')
    expect(target.uptime).toBe('01:01:01')
    expect(target.capabilities).toContain('wifi_config')
  })

  it('redacts wifi password before writing trace history', () => {
    const frame = createUsbWifiConfigFrame('req-1', {
      op: 'set',
      ssid: 'FluxPurr-Lab',
      password: 'secret-pass',
    })

    expect(redactWifiConfigFrame(frame)).toMatchObject({
      password: '<redacted>',
    })
    expect(JSON.stringify(redactWifiConfigFrame(frame))).not.toContain('secret-pass')
  })

  it('surfaces API error envelopes with retry metadata', async () => {
    const fetcher = vi.fn(async () => ({
      ok: false,
      status: 409,
      json: async () => ({
        error: {
          code: 'lease_conflict',
          message: 'Another client owns the active USB lease.',
          retryable: true,
        },
      }),
    })) as unknown as typeof fetch
    const client = createControlPlaneHttpClient(fetcher)

    await expect(client.createDevdLease('http://127.0.0.1:30080', 'mock')).rejects.toMatchObject({
      code: 'lease_conflict',
      retryable: true,
    })
  })

  it('probes devd device endpoints with the active lease', async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.endsWith('/identity?lease_id=lease-1')) {
        return {
          ok: true,
          json: async () => ({
            deviceId: 'frontpanel-1',
            firmwareVersion: '0.1.0',
            buildId: 'build-1',
            gitSha: 'abc',
            board: 'esp32-s3',
            apiVersion: '2026-05-23',
            protocolVersion: 'flux-purr.usb.v1',
            hostname: 'frontpanel-1',
            capabilities: ['identity', 'status'],
          }),
        }
      }
      if (url.endsWith('/network?lease_id=lease-1')) {
        return {
          ok: true,
          json: async () => ({
            state: 'idle',
            wifiRssi: null,
          }),
        }
      }
      return {
        ok: true,
        json: async () => ({
          mode: 'idle',
          uptimeSeconds: 4,
          currentTempC: 24,
          targetTempC: 220,
          heaterEnabled: false,
          heaterOutputPercent: 0,
          activeCoolingEnabled: true,
          fanDisplayState: 'OFF',
          fanEnabled: false,
          fanPwmPermille: 500,
          voltageMv: 5000,
          currentMa: 0,
          boardTempCenti: 2400,
          pdRequestMv: 20000,
          pdContractMv: 5000,
          pdState: 'fallback_5v',
          frontpanelKey: null,
          network: {
            state: 'idle',
            wifiRssi: null,
          },
        }),
      }
    }) as unknown as typeof fetch
    const client = createControlPlaneHttpClient(fetcher)

    const result = await client.probeDevdDevice(
      'http://127.0.0.1:30080',
      'native target',
      'lease-1'
    )

    expect(result.identity.deviceId).toBe('frontpanel-1')
    expect(fetcher).toHaveBeenCalledWith(
      'http://127.0.0.1:30080/api/v1/devices/native%20target/identity?lease_id=lease-1',
      undefined
    )
  })

  it('loads and verifies devd firmware artifacts', async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.endsWith('/api/v1/artifacts') && !init) {
        return {
          ok: true,
          json: async () => ({
            artifacts: [
              {
                artifactId: 'local-esp32s3-release',
                name: 'Local ESP32-S3 release',
                version: 'local-build',
                gitSha: 'abc',
                buildId: 'build-1',
                targetChip: 'esp32s3',
                profile: 'release + web_serial',
                features: ['web_serial'],
                protocol: 'flux-purr.usb.v1',
                files: [
                  {
                    kind: 'app',
                    path: 'firmware/target/xtensa-esp32s3-none-elf/release/flux-purr',
                    sha256: 'sha256:abc',
                    size: 42,
                    flashAddress: 65536,
                  },
                ],
              },
            ],
          }),
        }
      }
      return {
        ok: true,
        json: async () => ({
          artifactId: 'local-esp32s3-release',
          verified: true,
          files: [{ kind: 'app', sha256: 'sha256:abc', size: 42, ok: true }],
        }),
      }
    }) as unknown as typeof fetch
    const client = createControlPlaneHttpClient(fetcher)

    const artifacts = await client.listDevdArtifacts('http://127.0.0.1:30080')
    const manifest = artifactToManifest(artifacts[0])
    const result = await client.verifyArtifact('http://127.0.0.1:30080', manifest)

    expect(artifacts[0]).toMatchObject({
      id: 'local-esp32s3-release',
      compatibility: 'match',
      files: [{ size: 42 }],
    })
    expect(manifest.files[0].flashAddress).toBe(65536)
    expect(result.verified).toBe(true)
    expect(fetcher).toHaveBeenLastCalledWith(
      'http://127.0.0.1:30080/api/v1/artifacts/verify',
      expect.objectContaining({ method: 'POST' })
    )
  })

  it('marks non-esp32s3 catalog entries as blocked', () => {
    const artifact = manifestToArtifact({
      artifactId: 'host-release',
      name: 'Host release',
      version: 'local-build',
      gitSha: 'abc',
      buildId: 'build-1',
      targetChip: 'host',
      profile: 'host release',
      features: [],
      protocol: 'flux-purr.usb.v1',
      files: [],
    })

    expect(artifact.compatibility).toBe('blocked')
  })
})
