import { describe, expect, it, vi } from 'vitest'
import type { DevdDeviceRecord } from './contracts'
import {
  createControlPlaneHttpClient,
  createUsbWifiConfigFrame,
  devdRecordToDeviceTarget,
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
})
