import { describe, expect, it } from 'vitest'

import { rtdAdcMvForTemperature, rtdTemperatureForAdcMv } from './rtd-calibration-display'

describe('rtd calibration display helpers', () => {
  it('uses the nominal 3.328 V RTD divider excitation', () => {
    expect(rtdAdcMvForTemperature(31)).toBe(1033)
    expect(rtdTemperatureForAdcMv(1033)).toBeCloseTo(31, 0)
  })

  it('round-trips an elevated temperature through the divider model', () => {
    const targetMv = rtdAdcMvForTemperature(100)

    expect(targetMv).toBe(1190)
    expect(rtdTemperatureForAdcMv(targetMv)).toBeGreaterThan(99.9)
    expect(rtdTemperatureForAdcMv(targetMv)).toBeLessThan(100.3)
  })
})
