import { describe, expect, it, vi } from 'vitest'
import type { DevdDeviceRecord } from './contracts'
import { mergeDevdProbeRecord } from './live-devd'
import {
  artifactToManifest,
  createControlPlaneHttpClient,
  createUsbWifiConfigFrame,
  devdEventToLogEntry,
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
        apiVersion: '2026-05-29',
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
        selectedPresetSlot: 5,
        presetsC: [50, 100, 120, 150, 180, 220, 210, 230, 250, 300],
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
    expect(target.selectedPresetIndex).toBe(5)
    expect(target.presetsC?.[5]).toBe(220)
  })

  it('keeps daemon-local capabilities after a successful native firmware probe', () => {
    const record: DevdDeviceRecord = {
      id: 'serial-1',
      displayName: 'Authorized USB target',
      portPath: '/dev/cu.usbmodem21221401',
      transport: 'native_serial',
      connection: 'disconnected',
      identity: {
        deviceId: 'serial-1',
        firmwareVersion: 'devd-placeholder',
        buildId: 'devd',
        gitSha: 'unknown',
        board: 'esp32-s3',
        apiVersion: '2026-05-29',
        protocolVersion: 'flux-purr.usb.v1',
        hostname: 'serial-1',
        capabilities: ['identity', 'status', 'network', 'wifi_config', 'firmware_check', 'flash'],
      },
      network: {
        state: 'idle',
        wifiRssi: null,
      },
      status: {
        mode: 'idle',
        uptimeSeconds: 0,
        currentTempC: 0,
        targetTempC: 220,
        heaterEnabled: false,
        heaterOutputPercent: 0,
        activeCoolingEnabled: true,
        fanDisplayState: 'OFF',
        fanEnabled: false,
        fanPwmPermille: 1000,
        voltageMv: 5000,
        currentMa: 0,
        boardTempCenti: 0,
        pdRequestMv: 20000,
        pdContractMv: 5000,
        pdState: 'fallback_5v',
        frontpanelKey: null,
        network: {
          state: 'idle',
          wifiRssi: null,
        },
      },
    }

    const merged = mergeDevdProbeRecord(record, {
      identity: {
        deviceId: 'flux-purr-s3-001',
        firmwareVersion: '0.1.0',
        buildId: 'firmware-build',
        gitSha: 'abc',
        board: 'esp32-s3',
        apiVersion: '2026-05-29',
        protocolVersion: 'flux-purr.usb.v1',
        hostname: 'flux-purr-s3-001',
        capabilities: ['identity', 'status', 'network', 'usb_jsonl', 'wifi_config', 'monitor'],
      },
      network: {
        state: 'connected',
        ssid: 'FluxPurr-Lab',
        wifiRssi: -52,
      },
      status: {
        ...record.status,
        network: {
          state: 'connected',
          ssid: 'FluxPurr-Lab',
          wifiRssi: -52,
        },
      },
    })

    expect(merged.connection).toBe('connected')
    expect(merged.identity.deviceId).toBe('flux-purr-s3-001')
    expect(merged.identity.capabilities).toEqual([
      'identity',
      'status',
      'network',
      'wifi_config',
      'firmware_check',
      'flash',
      'usb_jsonl',
      'monitor',
    ])
    expect(devdRecordToDeviceTarget(merged).capabilities).toContain('flash')
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

  it('maps devd events into monitor trace entries without raw payload dumps', () => {
    const entry = devdEventToLogEntry({
      id: 'event-1',
      timestamp: '12345',
      deviceId: 'serial-1',
      kind: 'serial',
      message: 'native serial RPC failed',
      payload: {
        stage: 'identity',
        code: 'usb_response_timeout',
        message: 'Timed out waiting for a matching USB JSONL response.',
        retryable: true,
      },
    })

    expect(entry).toMatchObject({
      time: '12345',
      source: 'serial',
      tone: 'danger',
      message: 'native serial RPC failed: identity / usb_response_timeout',
    })
    expect(JSON.stringify(entry)).not.toContain('Timed out waiting')

    const wifiEntry = devdEventToLogEntry({
      id: 'event-2',
      timestamp: '12346',
      deviceId: 'serial-1',
      kind: 'wifi',
      message: 'wifi config accepted',
      payload: {
        ssid: 'FluxPurr-Lab',
        passwordPresent: true,
        password: 'secret-pass',
      },
    })
    expect(wifiEntry).toMatchObject({
      source: 'wifi',
      tone: 'success',
      message: 'wifi config accepted: FluxPurr-Lab / password present',
    })
    expect(JSON.stringify(wifiEntry)).not.toContain('secret-pass')

    const runtimeEntry = devdEventToLogEntry({
      id: 'event-3',
      timestamp: '12347',
      deviceId: 'serial-1',
      kind: 'runtime',
      message: 'runtime config applied',
      payload: {
        status: {
          targetTempC: 231,
          activeCoolingEnabled: false,
          heaterEnabled: false,
        },
      },
    })
    expect(runtimeEntry.tone).toBe('success')
    expect(runtimeEntry.message).toBe(
      'runtime config applied: target 231C / cooling off / heater off'
    )

    const partialRuntimeEntry = devdEventToLogEntry({
      id: 'event-4',
      timestamp: '12348',
      kind: 'runtime',
      message: 'runtime config applied',
      payload: { status: { targetTempC: null, heaterEnabled: true } },
    })
    expect(partialRuntimeEntry.message).toBe('runtime config applied: heater on')
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
    let inFlight = 0
    let maxInFlight = 0
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      inFlight += 1
      maxInFlight = Math.max(maxInFlight, inFlight)
      await new Promise((resolve) => setTimeout(resolve, 0))
      inFlight -= 1

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
            apiVersion: '2026-05-29',
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
    expect(maxInFlight).toBe(1)
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
                    kind: 'elf',
                    path: 'firmware/target/xtensa-esp32s3-none-elf/release/flux-purr',
                    sha256: 'sha256:abc',
                    size: 42,
                    flashAddress: null,
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
          files: [{ kind: 'elf', sha256: 'sha256:abc', size: 42, ok: true }],
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
    expect(manifest.files[0].kind).toBe('elf')
    expect(manifest.files[0].flashAddress).toBeNull()
    expect(result.verified).toBe(true)
    expect(fetcher).toHaveBeenLastCalledWith(
      'http://127.0.0.1:30080/api/v1/artifacts/verify',
      expect.objectContaining({ method: 'POST' })
    )
  })

  it('sends runtime, wifi, and flash mutations through devd endpoints', async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = []
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ url: String(input), init })
      const url = String(input)
      const body = init?.body ? JSON.parse(String(init.body)) : null

      if (url.endsWith('/runtime')) {
        expect(body).toMatchObject({
          leaseId: 'lease-1',
          targetTempC: 225,
          activeCoolingEnabled: false,
          heaterEnabled: true,
        })
        return {
          ok: true,
          json: async () => ({
            mode: 'sampling',
            uptimeSeconds: 8,
            currentTempC: 180,
            targetTempC: 225,
            heaterEnabled: true,
            heaterOutputPercent: 24,
            activeCoolingEnabled: false,
            fanDisplayState: 'OFF',
            fanEnabled: false,
            fanPwmPermille: 1000,
            voltageMv: 20000,
            currentMa: 850,
            boardTempCenti: 3700,
            pdRequestMv: 20000,
            pdContractMv: 20000,
            pdState: 'ready',
            frontpanelKey: null,
            network: { state: 'connected', ssid: 'FluxPurr-Lab', wifiRssi: -52 },
          }),
        }
      }

      if (url.endsWith('/wifi')) {
        expect(body).toMatchObject({
          leaseId: 'lease-1',
          op: 'set',
          ssid: 'FluxPurr-Lab',
          password: 'secret-pass',
          autoReconnect: true,
          telemetryIntervalMs: 500,
        })
        return {
          ok: true,
          json: async () => ({
            network: { state: 'saving', ssid: 'FluxPurr-Lab', wifiRssi: null },
            wifi: { password: '<redacted>' },
          }),
        }
      }

      expect(url).toMatch(/\/flash$/)
      expect(body).toMatchObject({
        leaseId: 'lease-1',
        dryRun: false,
        confirm: 'FLASH',
        artifact: { artifactId: 'local-esp32s3-release' },
      })
      return {
        ok: true,
        json: async () => ({
          artifactId: 'local-esp32s3-release',
          dryRun: false,
          status: 'completed',
          message: 'espflash command completed.',
        }),
      }
    }) as unknown as typeof fetch
    const client = createControlPlaneHttpClient(fetcher)
    const artifact = {
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
          kind: 'elf',
          path: 'firmware/target/xtensa-esp32s3-none-elf/release/flux-purr',
          sha256: 'sha256:abc',
          size: 42,
          flashAddress: null,
        },
      ],
    }

    const status = await client.configureRuntime('http://127.0.0.1:30080', 'native target', {
      leaseId: 'lease-1',
      targetTempC: 225,
      activeCoolingEnabled: false,
      heaterEnabled: true,
    })
    const network = await client.configureWifi('http://127.0.0.1:30080', 'native target', {
      leaseId: 'lease-1',
      op: 'set',
      ssid: 'FluxPurr-Lab',
      password: 'secret-pass',
      autoReconnect: true,
      telemetryIntervalMs: 500,
    })
    const flash = await client.flashDevice('http://127.0.0.1:30080', 'native target', {
      leaseId: 'lease-1',
      artifact,
      dryRun: false,
      confirm: 'FLASH',
    })

    expect(status.targetTempC).toBe(225)
    expect(network.state).toBe('saving')
    expect(flash.status).toBe('completed')
    expect(calls.map((call) => call.url)).toEqual([
      'http://127.0.0.1:30080/api/v1/devices/native%20target/runtime',
      'http://127.0.0.1:30080/api/v1/devices/native%20target/wifi',
      'http://127.0.0.1:30080/api/v1/devices/native%20target/flash',
    ])
    for (const call of calls) {
      expect(call.init).toMatchObject({
        method: expect.stringMatching(/PUT|POST/),
        headers: { 'content-type': 'application/json' },
      })
    }
  })

  it('sends daemon-local device mutations with the active lease', async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = []
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ url: String(input), init })

      return {
        ok: true,
        json: async () => ({
          id: 'native target',
          displayName: 'Bench Alias',
          portPath: '/dev/cu.usbmodem-test',
          transport: 'native_serial',
          connection: 'connected',
          identity: {
            deviceId: 'native target',
            firmwareVersion: '0.1.0',
            buildId: 'build-1',
            gitSha: 'abc',
            board: 'esp32-s3',
            apiVersion: '2026-05-29',
            protocolVersion: 'flux-purr.usb.v1',
            hostname: 'native-target',
            capabilities: ['identity', 'status', 'network'],
          },
          network: { state: 'idle', wifiRssi: null },
          status: {
            mode: 'idle',
            uptimeSeconds: 0,
            currentTempC: 0,
            targetTempC: 220,
            heaterEnabled: false,
            heaterOutputPercent: 0,
            activeCoolingEnabled: true,
            fanDisplayState: 'OFF',
            fanEnabled: false,
            fanPwmPermille: 0,
            voltageMv: 5000,
            currentMa: 0,
            boardTempCenti: 0,
            pdRequestMv: 20000,
            pdContractMv: 5000,
            pdState: 'fallback_5v',
            frontpanelKey: null,
            network: { state: 'idle', wifiRssi: null },
          },
        }),
      }
    }) as unknown as typeof fetch
    const client = createControlPlaneHttpClient(fetcher)

    await client.bindDevdDevice('http://127.0.0.1:30080', 'native target', 'lease-1', {
      alias: 'Bench Alias',
    })
    await client.connectDevdDevice('http://127.0.0.1:30080', 'native target', 'lease-1')
    await client.disconnectDevdDevice('http://127.0.0.1:30080', 'native target', 'lease-1')

    expect(calls.map((call) => call.url)).toEqual([
      'http://127.0.0.1:30080/api/v1/devices/native%20target/bind?lease_id=lease-1',
      'http://127.0.0.1:30080/api/v1/devices/native%20target/connect?lease_id=lease-1',
      'http://127.0.0.1:30080/api/v1/devices/native%20target/disconnect?lease_id=lease-1',
    ])
    expect(calls[0]?.init).toMatchObject({
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ alias: 'Bench Alias' }),
    })
    expect(calls[1]?.init).toMatchObject({ method: 'POST' })
    expect(calls[2]?.init).toMatchObject({ method: 'POST' })
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
