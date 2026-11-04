import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import {
  createDefaultThermalPlantSnapshot,
  createEmptyThermalPlantSnapshot,
  ThermalPlantRunCard,
  thermalPlantRunCardPresentation,
} from './thermal-plant-run-card'

describe('thermal plant run result presentation', () => {
  it('shows a live snapshot loading state without presenting it as idle', () => {
    const snapshot = createEmptyThermalPlantSnapshot()
    const markup = renderToStaticMarkup(
      createElement(ThermalPlantRunCard, { snapshot, loading: true, onStartStop: () => undefined })
    )

    expect(markup).toContain('aria-busy="true"')
    expect(markup).toContain('thermal-plant-run-card--loading')
    expect(markup).toContain('正在读取设备快照')
    expect(markup.match(/class="thermal-plant-run-card__chart-skeleton /g)).toHaveLength(2)
    expect(markup.match(/class="thermal-plant-run-card__chart-skeleton-canvas"/g)).toHaveLength(2)
    expect(markup.match(/class="thermal-plant-run-card__chart-skeleton-grid"/g)).toHaveLength(2)
    expect(markup.match(/class="thermal-plant-run-card__chart-skeleton-axes"/g)).toHaveLength(2)
    expect(markup.match(/class="thermal-plant-run-card__chart-skeleton-status"/g)).toHaveLength(2)
    expect(markup.match(/class="thermal-plant-run-card__chart-skeleton-plot"/g)).toHaveLength(2)
    expect(markup.match(/class="thermal-plant-run-card__table-loading-status"/g)).toHaveLength(1)
    expect(markup).not.toContain('thermal-plant-run-card__chart-skeleton-area')
    expect(markup).not.toContain('thermal-plant-run-card__chart-skeleton-line')
    expect(markup).not.toContain('thermal-plant-run-card__chart-skeleton-trace')
    expect(markup).not.toContain('<text')
    expect(markup).not.toContain('<circle')
    expect(markup).not.toContain('>0 点</span>')
    expect(markup.match(/thermal-plant-run-card__table-loading-body/g)).toHaveLength(1)
    expect(markup.match(/thermal-plant-run-card__table-loading-row/g)).toHaveLength(8)
    expect(markup.match(/class="thermal-plant-run-card__table-loading-body"/g)).toHaveLength(1)
    expect(markup).toContain('aria-label="读取中"')
    expect(markup).not.toContain('正在读取供电数据')
    expect(markup).not.toContain('正在读取温度数据')
    expect(markup).not.toContain('正在读取供电边界采样')
    expect(markup).toContain('thermal-plant-run-card__metric-value-skeleton')
    expect(markup).not.toContain('thermal-plant-run-card__state-summary')
    expect(markup).not.toContain('设备已就绪')
    expect(markup).not.toContain('尚未开始自动热模型标定')
  })

  it.each([
    { loading: true },
    { readError: 'snapshot read failed' },
  ])('keeps a known running job cancellable while its snapshot is unavailable: %j', (readState) => {
    const markup = renderToStaticMarkup(
      createElement(ThermalPlantRunCard, {
        snapshot: createEmptyThermalPlantSnapshot(),
        runtimeRunning: true,
        ...readState,
        onStartStop: () => undefined,
      })
    )

    expect(markup).toContain('停止自动校准')
    expect(markup).toContain('thermal-plant-run-card__command--stop')
    expect(markup).not.toContain('disabled')
    expect(markup).not.toContain('正在读取设备状态…')
  })

  it.each([
    {},
    { loading: true },
    { readError: 'snapshot read failed' },
  ])('uses an explicit terminal runtime state instead of a stale running snapshot: %j', (readState) => {
    const snapshot = createDefaultThermalPlantSnapshot()
    if (!snapshot.attempt) throw new Error('default fixture requires an attempt')
    snapshot.attempt = { ...snapshot.attempt, status: 'running', restartAllowed: false }
    const markup = renderToStaticMarkup(
      createElement(ThermalPlantRunCard, {
        snapshot,
        runtimeRunning: false,
        ...readState,
        onStartStop: () => undefined,
      })
    )
    const command = markup.match(
      /<button[^>]*class="thermal-plant-run-card__command[^"]*"[^>]*>[\s\S]*?<\/button>/
    )?.[0]

    expect(command).toBeDefined()
    expect(command).not.toContain('停止自动校准')
    expect(command).not.toContain('thermal-plant-run-card__command--stop')
    expect(command).toContain('disabled')
  })

  it('keeps an idle loading job and transport-blocked stop command disabled', () => {
    for (const props of [
      { loading: true },
      { loading: true, runtimeRunning: true, disabled: true },
    ]) {
      const markup = renderToStaticMarkup(
        createElement(ThermalPlantRunCard, {
          snapshot: createEmptyThermalPlantSnapshot(),
          ...props,
          onStartStop: () => undefined,
        })
      )
      expect(markup).toContain('disabled')
    }
  })

  it('labels an empty snapshot as ready to start instead of loading', () => {
    const snapshot = createEmptyThermalPlantSnapshot()
    const presentation = thermalPlantRunCardPresentation(snapshot)
    const markup = renderToStaticMarkup(
      createElement(ThermalPlantRunCard, { snapshot, onStartStop: () => undefined })
    )

    expect(presentation.statusText).toBe('待开始')
    expect(presentation.traceStatus).toBe('待开始')
    expect(presentation.runEvidence).toBe('设备已就绪 · 尚未开始自动热模型标定')
    expect(markup).toContain('设备已就绪')
    expect(markup).toContain('尚未开始采样')
    expect(markup).toContain('尚未采集供电边界数据')
    expect(markup).not.toContain('读取中')
    expect(markup).not.toContain('正在读取设备快照')
  })

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

  it('keeps dense boundary chart labels readable without stretching the SVG text', () => {
    const defaultSnapshot = createDefaultThermalPlantSnapshot()
    if (!defaultSnapshot.supplyBoundary)
      throw new Error('default fixture requires a supply boundary')
    const points = Array.from(
      { length: 17 },
      (_, index) =>
        [
          index * 1_000,
          1_000,
          1_000,
          5_000,
          5_000,
          3_000,
          13_500 + index * 500,
          13_500 + index * 500,
          0,
        ] as [number, number, number, number, number, number, number, number, number]
    )
    const snapshot = {
      ...defaultSnapshot,
      supplyBoundary: {
        ...defaultSnapshot.supplyBoundary,
        points,
      },
    }
    const markup = renderToStaticMarkup(
      createElement(ThermalPlantRunCard, { snapshot, onStartStop: () => undefined })
    )
    const boundarySvg = markup.match(
      /<svg class="thermal-plant-run-card__svg thermal-plant-run-card__svg--boundary"[\s\S]*?<\/svg>/
    )?.[0]
    if (!boundarySvg) throw new Error('boundary chart SVG is missing')
    const axisLabels = boundarySvg.match(/>\d+\.\dV<\/text>/g) ?? []

    expect(boundarySvg).toContain('preserveAspectRatio="xMidYMid meet"')
    expect(axisLabels).toHaveLength(6)
    expect(axisLabels[0]).toBe('>13.5V</text>')
    expect(axisLabels.at(-1)).toBe('>21.5V</text>')
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
