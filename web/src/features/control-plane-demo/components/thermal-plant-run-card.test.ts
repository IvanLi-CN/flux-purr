import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import {
  createDefaultThermalPlantSnapshot,
  ThermalPlantRunCard,
  thermalPlantRunCardPresentation,
} from './thermal-plant-run-card'

describe('thermal plant run result presentation', () => {
  it('presents the supply boundary and transient evidence instead of legacy resistance data', () => {
    const snapshot = createDefaultThermalPlantSnapshot()
    const markup = renderToStaticMarkup(
      createElement(ThermalPlantRunCard, { snapshot, onStartStop: () => undefined })
    )

    expect(markup).toContain('供电边界曲线')
    expect(markup).toContain('供电边界采样')
    expect(markup).toContain('请求电压')
    expect(markup).toContain('21.0 V')
    expect(markup).toContain('跟踪')
    expect(markup).not.toContain('R(T)')
  })

  it('explains that a live run is waiting for boundary data instead of showing a blank chart', () => {
    const defaultSnapshot = createDefaultThermalPlantSnapshot()
    if (!defaultSnapshot.attempt) throw new Error('default fixture requires an attempt')
    const snapshot = {
      ...defaultSnapshot,
      supplyBoundary: null,
      attempt: {
        ...defaultSnapshot.attempt,
        status: 'running' as const,
        restartAllowed: false,
      },
    }
    const markup = renderToStaticMarkup(
      createElement(ThermalPlantRunCard, { snapshot, onStartStop: () => undefined })
    )

    expect(markup).toContain('等待供电边界采样')
    expect(markup).toContain('采样中')
    expect(markup).not.toContain('等待 R(T) 预览')
  })

  it('keeps the stop command enabled while automatic calibration is running', () => {
    const defaultSnapshot = createDefaultThermalPlantSnapshot()
    if (!defaultSnapshot.attempt) throw new Error('default fixture requires an attempt')
    const snapshot = {
      ...defaultSnapshot,
      attempt: {
        ...defaultSnapshot.attempt,
        status: 'running' as const,
        restartAllowed: false,
      },
    }
    const markup = renderToStaticMarkup(
      createElement(ThermalPlantRunCard, { snapshot, onStartStop: () => undefined })
    )

    expect(markup).toContain('停止自动校准')
    expect(markup).not.toContain('disabled')
  })

  it('does not present a preserved active result as a failed attempt success', () => {
    const completed = createDefaultThermalPlantSnapshot()
    const attempt = completed.attempt
    if (!attempt) throw new Error('default fixture requires an attempt')
    const failed = {
      ...completed,
      attempt: {
        ...attempt,
        status: 'failed' as const,
        restartAllowed: true,
        error: '自然冷却阶段未达到 80℃，未覆盖已有 active 结果。',
      },
    }

    const presentation = thermalPlantRunCardPresentation(failed)
    const markup = renderToStaticMarkup(
      createElement(ThermalPlantRunCard, { snapshot: failed, onStartStop: () => undefined })
    )

    expect(presentation.statusText).toBe('失败')
    expect(presentation.traceStatus).toBe('失败')
    expect(presentation.runEvidence).toContain('本次未写入 EEPROM')
    expect(presentation.runEvidence).toContain('当前 active 保留')
    expect(markup).toContain('供电边界曲线')
    expect(markup).not.toContain('80℃自然冷却完成')
  })
})
