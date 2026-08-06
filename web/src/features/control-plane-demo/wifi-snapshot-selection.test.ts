import { describe, expect, it } from 'vitest'

import { shouldUseWifiReceipt } from './components/control-plane-demo'

describe('WiFi receipt selection', () => {
  it('keeps the device-confirmed connected snapshot when it has the same version as a saving receipt', () => {
    expect(
      shouldUseWifiReceipt(
        { configurationGeneration: 1, transitionSequence: 5 },
        { configurationGeneration: 1, transitionSequence: 5 }
      )
    ).toBe(false)
  })

  it('uses a receipt only while it is strictly newer than the device snapshot', () => {
    expect(
      shouldUseWifiReceipt(
        { configurationGeneration: 1, transitionSequence: 5 },
        { configurationGeneration: 1, transitionSequence: 6 }
      )
    ).toBe(true)
  })
})
