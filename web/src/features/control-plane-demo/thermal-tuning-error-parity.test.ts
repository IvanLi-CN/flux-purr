import { describe, expect, it, vi } from 'vitest'
import { authorizedLanRequest } from './lan-client'
import { createControlPlaneHttpClient } from './transport-client'

describe('thermal tuning HTTP error parity', () => {
  const adapters = {
    direct: (fetcher: typeof fetch) =>
      authorizedLanRequest(
        { baseUrl: 'http://192.168.1.10', token: 'a'.repeat(64) },
        'thermal-tuning',
        'GET',
        undefined,
        undefined,
        fetcher
      ),
    devd: (fetcher: typeof fetch) =>
      createControlPlaneHttpClient(fetcher).getThermalTuning(
        'http://127.0.0.1:4170',
        'devd-lan-device'
      ),
  }

  for (const [adapter, read] of Object.entries(adapters)) {
    it.each([
      [404, 'route_missing', 'thermal_tuning_unsupported', false],
      [400, 'thermal_tuning_unsupported', 'thermal_tuning_unsupported', false],
      [401, 'pairing_required', 'unauthorized', false],
      [401, 'thermal_tuning_unsupported', 'unauthorized', false],
      [503, 'device_busy', 'server_retryable', true],
    ] as const)(`${adapter}: classifies HTTP %s / %s independently of contradictory retry hints`, async (status, protocolCode, category, retryable) => {
      const fetcher = vi.fn<typeof fetch>().mockResolvedValue(
        new Response(
          JSON.stringify({
            error: { code: protocolCode, message: 'device response', retryable: true },
          }),
          { status }
        )
      )
      await expect(read(fetcher)).rejects.toMatchObject({
        category,
        protocolCode,
        retryable,
        status,
        operation: 'thermal_tuning_read',
        endpoint: expect.stringMatching(/\/thermal-tuning$/),
        ...(category === 'thermal_tuning_unsupported'
          ? { code: 'thermal_tuning_unsupported' }
          : category === 'unauthorized'
            ? { code: 'unauthorized' }
            : {}),
      })
      expect(fetcher).toHaveBeenCalledTimes(1)
    })
  }
})
