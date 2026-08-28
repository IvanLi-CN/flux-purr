import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import {
  createDefaultThermalPlantSnapshot,
  ThermalPlantRunCard,
  thermalPlantRunCardPresentation,
} from './thermal-plant-run-card'

describe('thermal plant run result presentation', () => {
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
    expect(markup).toContain('R(T) 当前 active')
    expect(markup).not.toContain('80℃自然冷却完成')
  })
})
