import { describe, expect, it } from 'vitest'
import { bootstrapDemoRuntime, type DemoRuntimeWindow } from './demo-runtime'

describe('demo runtime bootstrap', () => {
  it('keeps formal builds on the live runtime without a browser global', () => {
    expect(bootstrapDemoRuntime({ publicDemo: false })).toEqual({
      variant: 'live',
      mockOnly: false,
    })
  })

  it('fails closed when the public demo has no fetch boundary', () => {
    expect(() => bootstrapDemoRuntime({ publicDemo: true })).toThrow(
      'The public demo mock runtime is unavailable.'
    )
  })

  it('blocks live API and cross-origin requests before they reach fetch', async () => {
    let fetchCalls = 0
    const targetWindow = {
      fetch: (async () => {
        fetchCalls += 1
        return new Response('{}')
      }) as typeof window.fetch,
      location: {
        href: 'https://demo.test/devices/fp-lab-01/overview',
        origin: 'https://demo.test',
      },
      document: { documentElement: { dataset: {} } },
    } as unknown as DemoRuntimeWindow
    const runtime = bootstrapDemoRuntime({
      publicDemo: true,
      targetWindow,
    })

    await expect(targetWindow.fetch('/api/v1/devices')).rejects.toThrow(
      'Live requests are disabled in the public demo.'
    )
    await expect(targetWindow.fetch('https://device.test/status')).rejects.toThrow(
      'Live requests are disabled in the public demo.'
    )
    expect(fetchCalls).toBe(0)
    expect(targetWindow.document.documentElement.dataset.fluxPurrRuntime).toBe('demo-mock-only')
    runtime.restore?.()
  })
})
