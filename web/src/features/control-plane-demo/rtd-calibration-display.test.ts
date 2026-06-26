import { describe, expect, it } from 'vitest'
import { rtdAdcMvForTemperature, rtdTemperatureForAdcMv } from './rtd-calibration-display'

describe('rtd calibration display helpers', () => {
  it('round-trips RTD calibration target ADC millivolts back to approximately the same temperature', () => {
    const targetTempC = 49
    const targetMv = rtdAdcMvForTemperature(targetTempC)
    const displayTempC = rtdTemperatureForAdcMv(targetMv)

    expect(targetMv).toBeGreaterThan(0)
    expect(displayTempC).toBeCloseTo(targetTempC, 0)
  })
})
