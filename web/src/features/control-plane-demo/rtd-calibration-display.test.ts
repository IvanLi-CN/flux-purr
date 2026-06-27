import { describe, expect, it } from 'vitest'

import { rtdTemperatureForAdcMv } from './rtd-calibration-display'

describe('rtd calibration display helpers', () => {
  it('maps the current ambient hold target close to the live sample temperature', () => {
    expect(rtdTemperatureForAdcMv(917)).toBeGreaterThan(24)
    expect(rtdTemperatureForAdcMv(917)).toBeLessThan(25)
    expect(rtdTemperatureForAdcMv(918)).toBeGreaterThan(24.8)
    expect(rtdTemperatureForAdcMv(918)).toBeLessThan(26)
  })

  it('maps the elevated hold target to the heating setpoint range', () => {
    expect(rtdTemperatureForAdcMv(1000)).toBeGreaterThan(62)
    expect(rtdTemperatureForAdcMv(1000)).toBeLessThan(64)
  })
})
