import { describe, expect, it } from 'vitest'
import observed from './fixtures/supply-observations.json'
import { theoreticalRtdTemperatureForAdcMv } from './rtd-calibration-display'
import { fitTemperatureRequestCurves } from './temperature-request-fit'

describe('display-only temperature/request fits', () => {
  it('recovers a polynomial in a scaled domain', () => {
    const rows = [30, 60, 90, 120, 150].map((temperatureC) => ({
      temperatureC,
      requestV: 5 + temperatureC * 0.1 - temperatureC ** 2 * 0.0002,
      currentMa: 3000,
    }))
    const [fit] = fitTemperatureRequestCurves(rows)
    expect(fit.rmseV).toBeLessThan(1e-9)
    expect(fit.points[0].temperatureC).toBe(25)
    expect(fit.points.at(-1)?.temperatureC).toBe(220)
    expect(fit.validPoints[0].temperatureC).toBe(30)
    expect(fit.validPoints.at(-1)?.temperatureC).toBe(150)
    for (const point of fit.points)
      expect(point.requestV).toBeCloseTo(
        5 + point.temperatureC * 0.1 - point.temperatureC ** 2 * 0.0002,
        8
      )
  })
  it('fits the captured 3A points without joining the single 2.85A observation', () => {
    const rows = observed.points.map((point) => ({
      temperatureC: theoreticalRtdTemperatureForAdcMv(
        point[2],
        observed.adc.dividerSupplyMv
      ) as number,
      requestV: (point[6] || point[7]) / 1000,
      currentMa: point[5],
    }))
    const fits = fitTemperatureRequestCurves(rows)
    expect(fits).toHaveLength(1)
    expect(fits[0].currentMa).toBe(3000)
    expect(fits[0].rmseV).toBeLessThan(1)
    expect(fits[0].points[0].temperatureC).toBe(25)
    expect(fits[0].points.at(-1)?.temperatureC).toBe(220)
    expect(fits[0].validPoints[0].temperatureC).toBeCloseTo(29.8, 1)
    expect(fits[0].validPoints.at(-1)?.temperatureC).toBeCloseTo(162.1, 1)
  })
  it('keeps separate current classes and declines insufficient temperature coverage', () => {
    const rows = [30, 60, 90].flatMap((temperatureC) => [
      { temperatureC, requestV: 10, currentMa: 3000 },
      { temperatureC, requestV: 15, currentMa: 2850 },
    ])
    const fits = fitTemperatureRequestCurves(rows)
    expect(fits).toHaveLength(2)
    expect(fits[0].points[10].requestV).toBeCloseTo(10)
    expect(fits[1].points[10].requestV).toBeCloseTo(15)
    expect(fitTemperatureRequestCurves(rows.slice(0, 2))).toEqual([])
    expect(fitTemperatureRequestCurves(rows.map((row) => ({ ...row, temperatureC: 30 })))).toEqual(
      []
    )
  })
  it('evaluates the fitted function across expanded chart bounds', () => {
    const rows = [30, 60, 90].map((temperatureC) => ({
      temperatureC,
      requestV: 5 + temperatureC * 0.1,
      currentMa: 3000,
    }))
    const [fit] = fitTemperatureRequestCurves(rows, { minC: 10, maxC: 260 })
    expect(fit.points[0]).toEqual({ temperatureC: 10, requestV: expect.closeTo(6, 8) })
    expect(fit.points.at(-1)).toEqual({ temperatureC: 260, requestV: expect.closeTo(31, 8) })
  })
})
