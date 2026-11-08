import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import type { SupplyBoundary } from '../contracts'
import observed from '../fixtures/supply-observations.json'
import {
  rtdAdcMvForTemperature,
  theoreticalRtdTemperatureForAdcMv,
} from '../rtd-calibration-display'
import { SupplyObservationTable, TemperatureRequestChart } from './thermal-supply-observations'

const boundary = observed as SupplyBoundary

describe('raw RTD supply observations', () => {
  it('computes low temperature points with their own divider supply', () => {
    expect(theoreticalRtdTemperatureForAdcMv(1030, 3328)).toBeCloseTo(29.8, 1)
    expect(theoreticalRtdTemperatureForAdcMv(1098, 3328)).toBeCloseTo(58.3, 1)
    expect(theoreticalRtdTemperatureForAdcMv(1030, 3400)).not.toBeCloseTo(29.8, 1)
    expect(boundary.points[0][6]).toBe(9500)
    expect(boundary.points[3][6]).toBe(14200)
  })
  it.each([
    undefined,
    0,
    -1,
    Number.NaN,
    Number.POSITIVE_INFINITY,
  ])('rejects invalid divider metadata %s', (supply) => {
    expect(theoreticalRtdTemperatureForAdcMv(1030, supply)).toBeNull()
  })
  it.each([0, -1, 3328, 4000, Number.NaN])('rejects invalid or out of range RTD %s', (raw) => {
    expect(theoreticalRtdTemperatureForAdcMv(raw, 3328)).toBeNull()
  })
  it('keeps all 16 rows and independently selectable scatter points', () => {
    const props = { boundary, loading: false, selectedIndex: 0, onSelect: () => undefined }
    const chart = renderToStaticMarkup(
      createElement(TemperatureRequestChart, { ...props, running: false })
    )
    const table = renderToStaticMarkup(createElement(SupplyObservationTable, props))
    expect(chart.match(/role="button"/g)).toHaveLength(16)
    expect(chart).not.toContain('<polyline')
    expect(table.match(/aria-label="选择观测 /g)).toHaveLength(16)
    expect(table).toContain('29.8℃')
    expect(table).toContain('58.3℃')
    expect(table).toContain('2.85 A')
    expect(table).toContain('来源未标注')
  })
  it('keeps raw observations when temperature conversion is unavailable', () => {
    const invalid = { ...boundary, adc: { ...boundary.adc, dividerSupplyMv: 0 } }
    const table = renderToStaticMarkup(
      createElement(SupplyObservationTable, {
        boundary: invalid,
        loading: false,
        selectedIndex: 0,
        onSelect: () => undefined,
      })
    )
    expect(table.match(/不可换算/g)).toHaveLength(16)
    expect(table).toContain('1030 mV')
    expect(table).not.toContain('NaN')
  })
  it('defaults to 5.5–28 V and 25–220°C, extending both axes for outlying observations', () => {
    function renderChart(value: SupplyBoundary) {
      return renderToStaticMarkup(
        createElement(TemperatureRequestChart, {
          boundary: value,
          loading: false,
          running: false,
          selectedIndex: 0,
          onSelect: () => undefined,
        })
      )
    }
    const normal = renderChart(boundary)
    for (const label of ['5.5', '28.0', '25', '220']) {
      expect(normal).toContain(`>${label}</text>`)
    }
    const outliers: SupplyBoundary = {
      ...boundary,
      points: [
        [0, 0, rtdAdcMvForTemperature(-10, 3328), 0, 0, 3000, 4500, 0, 1],
        [1000, 0, rtdAdcMvForTemperature(260, 3328), 0, 0, 3000, 32000, 0, 1],
      ],
    }
    const expanded = renderChart(outliers)
    for (const label of ['4.5', '32.0']) {
      expect(expanded).toContain(`>${label}</text>`)
    }
    for (const point of outliers.points) {
      const temp = theoreticalRtdTemperatureForAdcMv(point[2], 3328)
      expect(expanded).toContain(`>${temp?.toFixed(1).replace(/\.0$/, '')}</text>`)
    }
  })
})
