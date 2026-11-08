import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { ScrollArea } from '@/components/ui/scroll-area'
import type { SupplyBoundary } from '../contracts'
import { theoreticalRtdTemperatureForAdcMv } from '../rtd-calibration-display'
import { fitTemperatureRequestCurves } from '../temperature-request-fit'
import {
  chartValueAtPointer,
  clampChartValue,
  interpolateLinear,
  tooltipPlacement,
} from './chart-interaction'
import { thermalChartStyle } from './thermal-chart-style'
import { useResponsiveSvgHeight } from './use-responsive-svg-height'

type Observation = SupplyBoundary['points'][number]
type ObservationProps = {
  boundary?: SupplyBoundary | null
  loading: boolean
  interactionKey?: string
  selectedIndex: number
  onSelect: (index: number) => void
}

function temperature(point: Observation, boundary?: SupplyBoundary | null) {
  return theoreticalRtdTemperatureForAdcMv(point[2], boundary?.adc?.dividerSupplyMv)
}

function temperatureText(point: Observation, boundary?: SupplyBoundary | null) {
  const value = temperature(point, boundary)
  return value === null ? '不可换算' : `${value.toFixed(1)}℃`
}

function voltage(point: Observation) {
  return (point[6] || point[7]) / 1000
}
function current(point: Observation) {
  return (point[5] / 1000).toFixed(2)
}
function description(point: Observation, boundary?: SupplyBoundary | null) {
  return `${temperatureText(point, boundary)} · 申请 ${voltage(point).toFixed(1)} V · 合同 ${current(point)} A · RTD ${point[2]} mV · ${(point[0] / 1000).toFixed(2)} s`
}

export function TemperatureRequestChart({
  boundary,
  loading,
  running,
  interactionKey,
  selectedIndex,
  onSelect,
}: ObservationProps & { running: boolean }) {
  const { width, initialHeight, inset } = thermalChartStyle
  const { containerRef, viewBoxHeight: height } = useResponsiveSvgHeight(width, initialHeight)
  const [hoverX, setHoverX] = useState<number | null>(null)
  const [isPinned, setIsPinned] = useState(false)
  const svgRef = useRef<SVGSVGElement | null>(null)
  const observations = boundary?.points ?? []
  const points = observations.flatMap((point, index) => {
    const temp = temperature(point, boundary)
    return temp === null ? [] : [{ point, index, temp }]
  })
  const minT = Math.min(25, ...points.map(({ temp }) => temp))
  const maxT = Math.max(220, ...points.map(({ temp }) => temp))
  const fits = fitTemperatureRequestCurves(
    points.map(({ point, temp }) => ({
      temperatureC: temp,
      requestV: voltage(point),
      currentMa: point[5],
    })),
    { minC: minT, maxC: maxT }
  )
  const unavailable = loading || points.length === 0
  useEffect(() => {
    if (!unavailable && interactionKey === undefined) return
    setHoverX(null)
    setIsPinned(false)
  }, [interactionKey, unavailable])
  useEffect(() => {
    if (!isPinned) return
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (svgRef.current?.contains(event.target as Node)) return
      setIsPinned(false)
      setHoverX(null)
    }
    document.addEventListener('pointerdown', closeOnOutsidePointer)
    return () => document.removeEventListener('pointerdown', closeOnOutsidePointer)
  }, [isPinned])
  const voltages = [
    ...points.map(({ point }) => voltage(point)),
    ...fits.flatMap((fit) => fit.points.map((point) => point.requestV)),
  ]
  const minV = Math.min(5.5, ...voltages)
  const maxV = Math.max(28, ...voltages)
  const left = inset.left
  const right = width - inset.right
  const bottom = height - inset.bottom
  const top = inset.top
  const plotWidth = right - left
  const plotHeight = bottom - top
  const updateHover = (event: { clientX: number; clientY: number }, svg: SVGSVGElement) => {
    const position = chartValueAtPointer(event, svg, width, height)
    setHoverX(clampChartValue(minV + ((position.x - left) / plotWidth) * (maxV - minV), minV, maxV))
  }
  const hoverPixelX = hoverX === null ? null : left + ((hoverX - minV) / (maxV - minV)) * plotWidth
  const hoverValues =
    hoverX === null
      ? []
      : fits.flatMap((fit) => {
          const value = interpolateLinear(
            fit.points.map((point) => ({ x: point.requestV, y: point.temperatureC })),
            hoverX
          )
          return value === null ? [] : [{ currentMa: fit.currentMa, temperatureC: value }]
        })
  const hoverPlacement = hoverPixelX === null ? 'center' : tooltipPlacement(hoverPixelX, width)
  return (
    <div className="thermal-supply-chart">
      <div ref={containerRef} className="thermal-plant-run-card__chart-canvas">
        {loading || points.length === 0 ? (
          <output className="thermal-supply-chart__placeholder">
            {loading
              ? '读取中'
              : observations.length
                ? 'RTD 参数无效，温度不可换算；原始行保留在采样表。'
                : running
                  ? '等待供电观测'
                  : '尚未采集供电观测'}
          </output>
        ) : (
          <svg
            ref={svgRef}
            viewBox={`0 0 ${width} ${height}`}
            className="thermal-plant-run-card__svg"
            aria-label="RTD 理论温度—申请电压散点图"
            role="img"
            // biome-ignore lint/a11y/noNoninteractiveTabindex: The SVG is an intentional keyboard chart surface.
            tabIndex={0}
            onPointerMove={(event) => updateHover(event, event.currentTarget)}
            onPointerDown={(event) => {
              updateHover(event, event.currentTarget)
              if (event.pointerType === 'mouse') setIsPinned(true)
            }}
            onPointerLeave={() => {
              if (!isPinned) setHoverX(null)
            }}
            onPointerUp={(event) => {
              if (event.pointerType !== 'mouse') {
                setIsPinned(false)
                setHoverX(null)
              }
            }}
            onPointerCancel={() => {
              setIsPinned(false)
              setHoverX(null)
            }}
            onKeyDown={(event) => {
              if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
              event.preventDefault()
              const step = (maxV - minV) / 80
              const selectedPoint = points.find((point) => point.index === selectedIndex)
              const next = hoverX ?? voltage(selectedPoint?.point ?? points[0].point)
              setHoverX(
                clampChartValue(next + (event.key === 'ArrowRight' ? step : -step), minV, maxV)
              )
            }}
          >
            <title>
              横轴：申请电压；纵轴：RTD 理论温度。圆点为 3.00
              A，菱形为其他合同限流；曲线为完整显示温区上的拟合函数，仅供审查。
              {fits
                .map(
                  (fit) => ` ${(fit.currentMa / 1000).toFixed(2)} A RMS ${fit.rmseV.toFixed(2)} V。`
                )
                .join('')}
            </title>
            <line
              x1={left}
              x2={left}
              y1={top}
              y2={bottom}
              className="thermal-plant-run-card__axis"
            />
            <line
              x1={left}
              x2={right}
              y1={bottom}
              y2={bottom}
              className="thermal-plant-run-card__axis"
            />
            {[0, 0.5, 1].map((fraction) => (
              <g key={fraction}>
                <text x={left - 8} y={bottom - fraction * (bottom - top)} textAnchor="end">
                  {(minT + fraction * (maxT - minT)).toFixed(1).replace(/\.0$/, '')}
                </text>
                <text x={left + fraction * (right - left)} y={bottom + 20} textAnchor="middle">
                  {(minV + (maxV - minV) * fraction).toFixed(1)}
                </text>
              </g>
            ))}
            <text x={left} y={18}>
              RTD 理论温度 / ℃
            </text>
            <text x={right} y={18} textAnchor="end" className="thermal-chart__key">
              ○ 3A · ◇ 其他 · — 拟合
            </text>
            {fits.map((fit) => (
              <path
                key={fit.currentMa}
                className="thermal-supply-chart__fit"
                aria-label={`${(fit.currentMa / 1000).toFixed(2)} A 温度—申请电压拟合曲线`}
                d={fit.points
                  .map(
                    (point, index) =>
                      `${index === 0 ? 'M' : 'L'} ${left + ((point.requestV - minV) / (maxV - minV)) * (right - left)} ${bottom - ((point.temperatureC - minT) / (maxT - minT)) * (bottom - top)}`
                  )
                  .join(' ')}
              />
            ))}
            {hoverPixelX !== null ? (
              <>
                <line
                  x1={hoverPixelX}
                  x2={hoverPixelX}
                  y1={top}
                  y2={bottom}
                  className="thermal-chart__hover-line"
                />
                {hoverValues.map((value) => (
                  <circle
                    key={value.currentMa}
                    cx={hoverPixelX}
                    cy={bottom - ((value.temperatureC - minT) / (maxT - minT)) * plotHeight}
                    r="4"
                    className="thermal-chart__hover-point"
                  />
                ))}
              </>
            ) : null}
            {points.map(({ point, index, temp }) => {
              const x = left + ((voltage(point) - minV) / (maxV - minV)) * (right - left)
              const y = bottom - ((temp - minT) / (maxT - minT)) * (bottom - top)
              return (
                // biome-ignore lint/a11y/useSemanticElements: SVG has no native button; this point implements focus, activation and pressed state.
                <g
                  key={`${point[0]}-${index}`}
                  role="button"
                  tabIndex={0}
                  aria-label={`观测 ${index + 1}：${description(point, boundary)}`}
                  aria-pressed={index === selectedIndex}
                  className="thermal-supply-chart__point"
                  onClick={() => onSelect(index)}
                  onFocus={() => onSelect(index)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault()
                      onSelect(index)
                    }
                  }}
                >
                  <circle cx={x} cy={y} r={10} style={{ fill: 'transparent', stroke: 'none' }} />
                  {point[5] === 3000 ? (
                    <circle cx={x} cy={y} r={index === selectedIndex ? 5 : 3} />
                  ) : (
                    <path d={`M ${x} ${y - 4} L ${x + 4} ${y} L ${x} ${y + 4} L ${x - 4} ${y} Z`} />
                  )}
                </g>
              )
            })}
          </svg>
        )}
        {!unavailable && hoverX !== null ? (
          <output
            className={`thermal-chart__tooltip thermal-chart__tooltip--${hoverPlacement}`}
            style={{ left: `${((hoverPixelX ?? left) / width) * 100}%` }}
            aria-live="polite"
          >
            <strong>申请 {hoverX.toFixed(1)} V</strong>
            {hoverValues.length > 0 ? (
              hoverValues.map((value) => (
                <span key={value.currentMa}>
                  {value.temperatureC.toFixed(1)}℃ · 合同 {(value.currentMa / 1000).toFixed(2)} A
                </span>
              ))
            ) : (
              <span>该位置无有效拟合数据</span>
            )}
          </output>
        ) : null}
      </div>
      <div className="thermal-chart__footer thermal-chart__footer--axis-only">
        <output className="thermal-chart__axis-title">申请电压 / V</output>
      </div>
    </div>
  )
}

export function SupplyObservationTable({
  boundary,
  loading,
  selectedIndex,
  onSelect,
  source,
  selectionIdentity,
}: ObservationProps & {
  source?: 'attempt' | 'active' | null
  selectionIdentity?: string
}) {
  const points = boundary?.points ?? []
  const selectedRowRef = useRef<HTMLTableRowElement | null>(null)
  // biome-ignore lint/correctness/useExhaustiveDependencies: boundary points can mount after the selected index.
  useLayoutEffect(() => {
    const row = selectedRowRef.current
    if (!row) return
    const scrollContainer = row
      .closest<HTMLElement>('.thermal-plant-run-card__table-scroll')
      ?.querySelector<HTMLElement>('.simplebar-content-wrapper')
    if (!scrollContainer) return

    const frame = window.requestAnimationFrame(() => {
      const rowRect = row.getBoundingClientRect()
      const containerRect = scrollContainer.getBoundingClientRect()
      const stickyHeaderBottom = scrollContainer
        .querySelector<HTMLElement>('thead')
        ?.getBoundingClientRect().bottom
      const visibleTop = Math.max(containerRect.top, stickyHeaderBottom ?? containerRect.top)
      const topDelta = rowRect.top - visibleTop
      const bottomDelta = rowRect.bottom - containerRect.bottom
      if (topDelta >= 0 && bottomDelta <= 0) return

      const maxScrollTop = Math.max(0, scrollContainer.scrollHeight - scrollContainer.clientHeight)
      const nextScrollTop = Math.min(
        maxScrollTop,
        Math.max(0, scrollContainer.scrollTop + (topDelta < 0 ? topDelta : bottomDelta))
      )
      const behavior = window.matchMedia('(prefers-reduced-motion: reduce)').matches
        ? 'auto'
        : 'smooth'
      if (typeof scrollContainer.scrollTo === 'function') {
        scrollContainer.scrollTo({ top: nextScrollTop, behavior })
      } else {
        scrollContainer.scrollTop = nextScrollTop
      }
    })

    return () => window.cancelAnimationFrame(frame)
  }, [points.length, selectedIndex, selectionIdentity])
  return (
    <section className="thermal-plant-run-card__table-wrap" aria-label="供电边界采样">
      <div className="thermal-plant-run-card__section-heading">
        <h3>供电观测</h3>
        <span>{loading ? '读取中' : `${points.length} 点`}</span>
      </div>
      <p className="thermal-supply-source">
        {source === 'attempt'
          ? '来源：当前尝试（含失败采样）'
          : source === 'active'
            ? '来源：有效 active 结果'
            : '来源未标注'}
      </p>
      <ScrollArea className="thermal-plant-run-card__table-scroll" autoHide={false}>
        <table aria-label="供电边界采样">
          <thead>
            <tr>
              <th>理论温度</th>
              <th>申请电压</th>
              <th>限流</th>
              <th>VIN</th>
              <th>RTD 原始</th>
              <th>类型</th>
            </tr>
          </thead>
          <tbody>
            {points.map((point, index) => (
              <tr
                key={`${point[0]}-${index}`}
                ref={selectedIndex === index ? selectedRowRef : undefined}
                aria-selected={selectedIndex === index}
              >
                <td>
                  <button
                    type="button"
                    aria-label={`选择观测 ${index + 1}`}
                    aria-pressed={selectedIndex === index}
                    onClick={() => onSelect(index)}
                  >
                    {temperatureText(point, boundary)}
                  </button>
                </td>
                <td>{voltage(point).toFixed(1)} V</td>
                <td>{current(point)} A</td>
                <td>{(point[4] / 1000).toFixed(1)} V</td>
                <td>{point[2]} mV</td>
                <td>{point[8] ? '跟踪' : '边界'}</td>
              </tr>
            ))}
            {points.length === 0 ? (
              <tr>
                <td colSpan={6}>{loading ? '读取中' : '尚未采集供电观测'}</td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </ScrollArea>
    </section>
  )
}
