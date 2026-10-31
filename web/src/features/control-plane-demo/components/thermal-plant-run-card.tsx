import {
  Activity,
  CircleStop,
  Gauge,
  type LucideIcon,
  Microscope,
  RefreshCw,
  Thermometer,
  TimerReset,
  Zap,
} from 'lucide-react'
import { useCallback, useLayoutEffect, useState } from 'react'
import type { SupplyBoundary, ThermalPlantRunSnapshot, ThermalPlantTracePoint } from '../contracts'

const phaseLabel: Record<ThermalPlantTracePoint['phase'], string> = {
  ambient: '环境',
  heating: '加热',
  cooling: '自然冷却',
}

const statusLabel: Record<string, string> = {
  idle: '待开始',
  running: '运行中',
  completed: '已完成',
  failed: '失败',
  canceled: '已停止',
}

export function thermalPlantRunCardPresentation(
  snapshot: ThermalPlantRunSnapshot,
  unsupported = false
) {
  const attempt = snapshot.attempt
  const isRunning = attempt?.status === 'running'
  const status = unsupported ? 'unsupported' : (attempt?.status ?? 'idle')
  const terminalFailure = status === 'failed' || status === 'canceled'
  const showActiveResult = Boolean(snapshot.activeResult) && !isRunning
  const preservesPriorActive = showActiveResult && terminalFailure
  const trace = snapshot.tracePage.points
  const statusText = unsupported
    ? '兼容状态'
    : terminalFailure
      ? (statusLabel[status] ?? status)
      : showActiveResult
        ? 'active 有效'
        : (statusLabel[status] ?? status)
  const runEvidence = isRunning
    ? `${phaseLabel[attempt?.phase ?? 'ambient']}中 · ${attempt?.sampleCount ?? trace.length} / ${Math.max(snapshot.tracePage.totalSamples, attempt?.sampleCount ?? 0)} 样本`
    : terminalFailure
      ? `${statusLabel[status]} · 本次未写入 EEPROM${attempt?.error ? ` · ${attempt.error}` : ''}${preservesPriorActive ? ' · 当前 active 保留' : ''}`
      : showActiveResult
        ? `${trace.length} / ${snapshot.tracePage.totalSamples} 瞬态样本 · 220℃断热 · 80℃自然冷却完成`
        : '等待开始自动热模型标定'
  const traceStatus = isRunning
    ? '采样中'
    : attempt?.status === 'completed'
      ? '80℃完成'
      : terminalFailure
        ? (statusLabel[status] ?? status)
        : trace.length > 0
          ? '已记录'
          : '等待中'

  return {
    isRunning,
    showActiveResult,
    preservesPriorActive,
    status,
    statusText,
    runEvidence,
    traceStatus,
  }
}

export function ThermalPlantRunCard({
  snapshot,
  disabled,
  unsupported = false,
  onStartStop,
}: {
  snapshot: ThermalPlantRunSnapshot
  disabled?: boolean
  unsupported?: boolean
  onStartStop: () => void
}) {
  const attempt = snapshot.attempt
  const presentation = thermalPlantRunCardPresentation(snapshot, unsupported)
  const { isRunning, showActiveResult } = presentation
  const boundary = snapshot.supplyBoundary
  const boundaryPoints = boundary?.points ?? []
  const trace = snapshot.tracePage.points
  const canControl = !unsupported && (isRunning || attempt == null || attempt.restartAllowed)

  return (
    <article className="thermal-plant-run-card" aria-label="自动热模型标定结果">
      <header className="thermal-plant-run-card__header">
        <output
          className={`thermal-plant-run-card__status thermal-plant-run-card__status--${presentation.status}`}
        >
          <span aria-hidden="true" />
          {presentation.statusText}
        </output>
      </header>

      {unsupported ? (
        <p className="thermal-plant-run-card__compatibility">
          当前固件未提供运行快照，保留通用自动校准状态读取。
        </p>
      ) : null}

      <div className="thermal-plant-run-card__metrics">
        {showActiveResult && snapshot.activeResult ? (
          <>
            <Metric
              icon={Gauge}
              label="热容"
              value={`${formatThermalCapacity(snapshot.activeResult.thermalCapacityMjPerC)} J/℃`}
            />
            <Metric
              icon={Zap}
              label="对流"
              value={`${formatOptional(snapshot.activeResult.convectionMwPerC, 0)} mW/℃`}
            />
            <Metric
              icon={Microscope}
              label="辐射"
              value={`${formatRadiation(snapshot.activeResult.radiationMwPerK4)} μW/K⁴`}
            />
            <Metric
              icon={TimerReset}
              label="延迟"
              value={`${formatDelay(snapshot.activeResult.transportDelayMs)} s`}
            />
          </>
        ) : (
          <>
            <Metric
              icon={Thermometer}
              label="阶段"
              value={phaseLabel[attempt?.phase ?? 'ambient']}
            />
            <Metric icon={Activity} label="进度" value={`${attempt?.progressPercent ?? 0}%`} />
            <Metric
              icon={Zap}
              label="当前温度"
              value={`${formatTemp(attempt?.currentTempCentiC)}℃`}
            />
            <Metric
              icon={Gauge}
              label="采样"
              value={`${attempt?.sampleCount ?? trace.length} 点`}
            />
          </>
        )}
      </div>

      <div className="thermal-plant-run-card__body">
        <div className="thermal-plant-run-card__charts">
          <section
            className="thermal-plant-run-card__chart thermal-plant-run-card__chart--boundary"
            aria-label="供电边界曲线"
          >
            <div className="thermal-plant-run-card__section-heading">
              <h3>供电边界曲线</h3>
              <span>{boundaryPoints.length} 点</span>
            </div>
            <SupplyBoundaryChart boundary={boundary} />
          </section>

          <section className="thermal-plant-run-card__trace-panel" aria-label="温度轨迹">
            <div className="thermal-plant-run-card__section-heading">
              <h3>温度轨迹</h3>
              <span>{presentation.traceStatus}</span>
            </div>
            <TraceChart points={trace} />
            <div className="thermal-plant-run-card__legend">
              <span>
                <i className="thermal-plant-run-card__legend-dot thermal-plant-run-card__legend-dot--ambient" />
                环境
              </span>
              <span>
                <i className="thermal-plant-run-card__legend-dot thermal-plant-run-card__legend-dot--heating" />
                加热
              </span>
              <span>
                <i className="thermal-plant-run-card__legend-dot thermal-plant-run-card__legend-dot--cooling" />
                自然冷却
              </span>
            </div>
          </section>
        </div>

        <aside className="thermal-plant-run-card__evidence" aria-label="瞬态与供电边界证据">
          <section className="thermal-plant-run-card__table-wrap" aria-label="供电边界采样">
            <div className="thermal-plant-run-card__section-heading">
              <h3>供电边界采样</h3>
              <span>{boundaryPoints.length} 点</span>
            </div>
            <div className="thermal-plant-run-card__table-scroll">
              <table aria-label="供电边界采样">
                <thead>
                  <tr>
                    <th>请求电压</th>
                    <th>限流</th>
                    <th>VIN</th>
                    <th>RTD 原始</th>
                    <th>类型</th>
                  </tr>
                </thead>
                <tbody>
                  {boundaryPoints.slice(-8).map((point, index) => (
                    <tr key={`${point[0]}-${point[6]}-${index}`}>
                      <td>{formatBoundaryVoltage(point)} V</td>
                      <td>{formatBoundaryCurrent(point)} A</td>
                      <td>{formatBoundaryVoltageMv(point[4])} V</td>
                      <td>{point[2]} mV</td>
                      <td>
                        <span className="thermal-plant-run-card__valid">
                          {point[8] ? '跟踪' : '边界'}
                        </span>
                      </td>
                    </tr>
                  ))}
                  {boundaryPoints.length === 0 ? (
                    <tr>
                      <td colSpan={5} className="thermal-plant-run-card__empty">
                        {presentation.isRunning ? '等待供电边界扫描' : '暂无供电边界采样'}
                      </td>
                    </tr>
                  ) : null}
                </tbody>
              </table>
            </div>
          </section>

          <footer className="thermal-plant-run-card__footer">
            <output className="thermal-plant-run-card__run-evidence" aria-live="polite">
              <Activity size={15} aria-hidden="true" />
              {presentation.runEvidence}
            </output>
            <button
              type="button"
              className={
                isRunning
                  ? 'thermal-plant-run-card__command thermal-plant-run-card__command--stop'
                  : 'thermal-plant-run-card__command'
              }
              disabled={disabled || !canControl}
              onClick={onStartStop}
            >
              {isRunning ? (
                <CircleStop size={16} aria-hidden="true" />
              ) : (
                <RefreshCw size={16} aria-hidden="true" />
              )}
              {isRunning ? '停止自动校准' : attempt == null ? '开始自动校准' : '重新开始自动校准'}
            </button>
          </footer>
        </aside>
      </div>
    </article>
  )
}

export function createDefaultThermalPlantSnapshot(): ThermalPlantRunSnapshot {
  const temperatures = [25, 35, 52, 78, 112, 148, 182, 207, 220, 205, 174, 138, 102, 80]
  return {
    supplyBoundary: createDefaultSupplyBoundary(),
    version: 2,
    attempt: {
      runId: 7,
      status: 'completed',
      phase: 'cooling',
      progressPercent: 100,
      elapsedMs: 420000,
      currentTempCentiC: 8000,
      heaterVoltageMv: 0,
      dutyPercent: 0,
      sampleCount: temperatures.length,
      restartAllowed: true,
      error: null,
    },
    tracePage: {
      startSample: 0,
      nextSample: null,
      totalSamples: temperatures.length,
      points: temperatures.map((temperatureC, sampleIndex) => ({
        sampleIndex,
        elapsedMs: sampleIndex * 30000,
        temperatureCentiC: temperatureC * 100,
        heaterVoltageMv: sampleIndex < 9 ? 21000 : 0,
        dutyPercent: sampleIndex < 9 ? 100 : 0,
        phase: sampleIndex === 0 ? 'ambient' : sampleIndex < 9 ? 'heating' : 'cooling',
      })),
    },
    provisionalCurve: null,
    activeResult: {
      transactionId: 7,
      curve: { points: [null, null, null, null, null, null, null, null] },
      convectionMwPerC: 120,
      radiationMwPerK4: 0.0002,
      thermalCapacityMjPerC: 42000,
      transportDelayMs: 500,
    },
  }
}

function createDefaultSupplyBoundary(): SupplyBoundary {
  const points = [
    [0, 1050, 1050, 5000, 5002, 3000, 5000, 5000, 0],
    [30_000, 1090, 1090, 9000, 9004, 3000, 9000, 9000, 0],
    [60_000, 1140, 1140, 13000, 13008, 3000, 13000, 13000, 0],
    [90_000, 1210, 1210, 17000, 17012, 3000, 17000, 17000, 0],
    [120_000, 1290, 1290, 21000, 21018, 3000, 21000, 21000, 1],
  ] as SupplyBoundary['points']
  return {
    formatVersion: 6,
    adc: {
      dividerSupplyMv: 3328,
      referenceMv: 1100,
      referenceCode: 2048,
      initCode: 2048,
      efuseVersion: 2,
      attenuationDb: 6,
    },
    wifiStarted: false,
    cutoffReached: true,
    sourceMinMv: 5000,
    sourceMaxMv: 21000,
    sourceMaxMa: 3000,
    sourcePowerLimitMw: 63_000,
    advertisedMinMv: 5000,
    advertisedMaxMv: 21000,
    points,
  }
}

export function createEmptyThermalPlantSnapshot(): ThermalPlantRunSnapshot {
  return {
    version: 1,
    attempt: null,
    tracePage: {
      startSample: 0,
      nextSample: null,
      totalSamples: 0,
      points: [],
    },
    provisionalCurve: null,
    activeResult: null,
  }
}

function Metric({ icon: Icon, label, value }: { icon: LucideIcon; label: string; value: string }) {
  return (
    <div className="thermal-plant-run-card__metric">
      <Icon size={16} aria-hidden="true" />
      <span>
        <small>{label}</small>
        <strong>{value}</strong>
      </span>
    </div>
  )
}

function TraceChart({ points }: { points: ThermalPlantTracePoint[] }) {
  const width = 440
  const initialHeight = 152
  const { containerRef, viewBoxHeight } = useResponsiveSvgHeight(width, initialHeight)
  if (points.length < 2)
    return (
      <div className="thermal-plant-run-card__chart-empty thermal-plant-run-card__chart-empty--trace">
        等待温度轨迹
      </div>
    )
  const height = viewBoxHeight
  const inset = {
    top: Math.max(12, height * 0.079),
    right: 18,
    bottom: Math.max(28, height * 0.184),
    left: 34,
  }
  const minT = Math.min(...points.map((point) => point.temperatureCentiC / 100), 25)
  const maxT = Math.max(...points.map((point) => point.temperatureCentiC / 100), 220)
  const maxElapsed = Math.max(...points.map((point) => point.elapsedMs), 1)
  const plotWidth = width - inset.left - inset.right
  const plotBottom = height - inset.bottom
  const plotHeight = plotBottom - inset.top
  const pointPosition = (point: ThermalPlantTracePoint) => ({
    x: inset.left + (point.elapsedMs / maxElapsed) * plotWidth,
    y:
      plotBottom - ((point.temperatureCentiC / 100 - minT) / Math.max(maxT - minT, 1)) * plotHeight,
  })
  const positions = points.map(pointPosition)
  const heatingStart = points.findIndex((point) => point.phase === 'heating')
  const coolingStart = points.findIndex((point) => point.phase === 'cooling')
  const ambient = positions.slice(0, Math.max(1, heatingStart))
  const heating = positions.slice(
    Math.max(0, heatingStart - 1),
    coolingStart === -1 ? positions.length : coolingStart
  )
  const cooling = coolingStart === -1 ? [] : positions.slice(Math.max(0, coolingStart - 1))
  const toPolyline = (segment: Array<{ x: number; y: number }>) =>
    segment.map((point) => `${point.x},${point.y}`).join(' ')
  const cutoffIndex = coolingStart === -1 ? points.length - 1 : Math.max(0, coolingStart - 1)
  const cutoffPoint = positions[cutoffIndex]
  const completionPoint = positions[positions.length - 1]
  const tickLabel = (temperature: number) =>
    Math.round(plotBottom - ((temperature - minT) / Math.max(maxT - minT, 1)) * plotHeight)
  return (
    <div ref={containerRef} className="thermal-plant-run-card__chart-canvas">
      <svg
        className="thermal-plant-run-card__svg thermal-plant-run-card__svg--trace"
        preserveAspectRatio="xMidYMid meet"
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label="加热和自然冷却温度曲线"
      >
        <line
          x1={inset.left}
          y1={inset.top}
          x2={inset.left}
          y2={plotBottom}
          className="thermal-plant-run-card__axis"
        />
        <line
          x1={inset.left}
          y1={plotBottom}
          x2={width - inset.right}
          y2={plotBottom}
          className="thermal-plant-run-card__axis"
        />
        <line
          x1={inset.left}
          y1={tickLabel(80)}
          x2={width - inset.right}
          y2={tickLabel(80)}
          className="thermal-plant-run-card__guide"
        />
        {cutoffPoint ? (
          <line
            x1={cutoffPoint.x}
            y1={inset.top}
            x2={cutoffPoint.x}
            y2={plotBottom}
            className="thermal-plant-run-card__cutoff"
          />
        ) : null}
        {ambient.length > 1 ? (
          <polyline
            points={toPolyline(ambient)}
            className="thermal-plant-run-card__trace thermal-plant-run-card__trace--ambient"
          />
        ) : null}
        {heating.length > 1 ? (
          <polyline
            points={toPolyline(heating)}
            className="thermal-plant-run-card__trace thermal-plant-run-card__trace--heating"
          />
        ) : null}
        {cooling.length > 1 ? (
          <polyline
            points={toPolyline(cooling)}
            className="thermal-plant-run-card__trace thermal-plant-run-card__trace--cooling"
          />
        ) : null}
        {cutoffPoint ? (
          <circle
            cx={cutoffPoint.x}
            cy={cutoffPoint.y}
            r="3.5"
            className="thermal-plant-run-card__point thermal-plant-run-card__point--heating"
          />
        ) : null}
        {completionPoint ? (
          <circle
            cx={completionPoint.x}
            cy={completionPoint.y}
            r="3.5"
            className="thermal-plant-run-card__point thermal-plant-run-card__point--cooling"
          />
        ) : null}
        <text x={inset.left - 8} y={tickLabel(maxT) + 3} textAnchor="end">
          {Math.round(maxT)}
        </text>
        <text x={inset.left - 8} y={tickLabel(80) + 3} textAnchor="end">
          80
        </text>
        <text x={inset.left - 8} y={plotBottom + 3} textAnchor="end">
          {Math.round(minT)}
        </text>
        <text x={inset.left} y={height - 5}>
          {formatElapsed(0)}
        </text>
        {cutoffPoint ? (
          <text x={cutoffPoint.x} y={height - 5} textAnchor="middle">
            {formatElapsed(points[cutoffIndex]?.elapsedMs ?? 0)}
          </text>
        ) : null}
        <text x={width - inset.right} y={height - 5} textAnchor="end">
          {formatElapsed(maxElapsed)}
        </text>
      </svg>
    </div>
  )
}

function SupplyBoundaryChart({ boundary }: { boundary?: SupplyBoundary | null }) {
  const points = (boundary?.points ?? []).map((point) => ({
    requestVoltageMv: point[6] || point[7],
    requestCurrentMa: point[5],
  }))
  const width = 600
  const initialHeight = 340
  const { containerRef, viewBoxHeight } = useResponsiveSvgHeight(width, initialHeight)
  if (points.length === 0)
    return (
      <div className="thermal-plant-run-card__chart-empty thermal-plant-run-card__chart-empty--boundary">
        等待供电边界采样
      </div>
    )
  const height = viewBoxHeight
  const inset = {
    top: Math.max(16, height * 0.047),
    right: 26,
    bottom: Math.max(36, height * 0.106),
    left: 38,
  }
  const minX = Math.min(...points.map((point) => point.requestVoltageMv))
  const maxX = Math.max(...points.map((point) => point.requestVoltageMv))
  const rawMaxY = Math.max(...points.map((point) => point.requestCurrentMa))
  const minY = 0
  const maxY = Math.max(boundary?.sourceMaxMa ?? 0, rawMaxY, 1)
  const plotWidth = width - inset.left - inset.right
  const plotBottom = height - inset.bottom
  const plotHeight = plotBottom - inset.top
  const positionFor = (point: { requestVoltageMv: number; requestCurrentMa: number }) => ({
    x: inset.left + ((point.requestVoltageMv - minX) / Math.max(maxX - minX, 1)) * plotWidth,
    y: plotBottom - ((point.requestCurrentMa - minY) / Math.max(maxY - minY, 1)) * plotHeight,
  })
  const positions = points.map(positionFor)
  const path = points
    .map((point) => {
      const position = positionFor(point)
      return `${position.x},${position.y}`
    })
    .join(' ')
  const tickIndices = selectBoundaryTickIndices(points)
  return (
    <div ref={containerRef} className="thermal-plant-run-card__chart-canvas">
      <svg
        className="thermal-plant-run-card__svg thermal-plant-run-card__svg--boundary"
        preserveAspectRatio="xMidYMid meet"
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label="供电边界曲线"
      >
        <line
          x1={inset.left}
          y1={inset.top}
          x2={inset.left}
          y2={plotBottom}
          className="thermal-plant-run-card__axis"
        />
        <line
          x1={inset.left}
          y1={plotBottom}
          x2={width - inset.right}
          y2={plotBottom}
          className="thermal-plant-run-card__axis"
        />
        <line
          x1={inset.left}
          y1={inset.top + plotHeight / 3}
          x2={width - inset.right}
          y2={inset.top + plotHeight / 3}
          className="thermal-plant-run-card__guide"
        />
        <line
          x1={inset.left}
          y1={inset.top + (plotHeight * 2) / 3}
          x2={width - inset.right}
          y2={inset.top + (plotHeight * 2) / 3}
          className="thermal-plant-run-card__guide"
        />
        <polyline points={path} className="thermal-plant-run-card__boundary" />
        {points.map((point, index) => (
          <circle
            key={`${point.requestVoltageMv}-${point.requestCurrentMa}-${index}`}
            cx={positions[index]?.x}
            cy={positions[index]?.y}
            r="4"
            className="thermal-plant-run-card__point thermal-plant-run-card__point--boundary"
          />
        ))}
        {tickIndices.map((index) => {
          const point = points[index]
          return (
            <text
              key={`${point.requestVoltageMv}-label-${index}`}
              x={positions[index]?.x}
              y={height - 8}
              textAnchor="middle"
            >
              {formatBoundaryVoltageMv(point.requestVoltageMv)}V
            </text>
          )
        })}
        <text x={inset.left - 16} y={inset.top + 5}>
          {formatBoundaryCurrentMa(maxY)}A
        </text>
        <text x={inset.left - 16} y={plotBottom + 3}>
          0A
        </text>
      </svg>
    </div>
  )
}

function useResponsiveSvgHeight(viewBoxWidth: number, initialHeight: number) {
  const [container, setContainer] = useState<HTMLDivElement | null>(null)
  const [viewBoxHeight, setViewBoxHeight] = useState(initialHeight)
  const containerRef = useCallback((node: HTMLDivElement | null) => setContainer(node), [])

  useLayoutEffect(() => {
    if (!container) return

    const updateViewBox = () => {
      const { width, height } = container.getBoundingClientRect()
      if (width <= 0 || height <= 0) return
      const nextHeight = (viewBoxWidth * height) / width
      setViewBoxHeight((currentHeight) =>
        Math.abs(currentHeight - nextHeight) < 0.5 ? currentHeight : nextHeight
      )
    }

    let frameId = 0
    const scheduleUpdate = () => {
      updateViewBox()
      if (typeof requestAnimationFrame !== 'function') return
      if (frameId !== 0) cancelAnimationFrame(frameId)
      frameId = requestAnimationFrame(() => {
        frameId = 0
        updateViewBox()
      })
    }

    const observer =
      typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(scheduleUpdate)

    updateViewBox()
    scheduleUpdate()
    observer?.observe(container)
    window.addEventListener('resize', scheduleUpdate)
    window.visualViewport?.addEventListener('resize', scheduleUpdate)
    return () => {
      observer?.disconnect()
      if (frameId !== 0) cancelAnimationFrame(frameId)
      window.removeEventListener('resize', scheduleUpdate)
      window.visualViewport?.removeEventListener('resize', scheduleUpdate)
    }
  }, [container, viewBoxWidth])

  return { containerRef, viewBoxHeight }
}

function selectBoundaryTickIndices(points: Array<{ requestVoltageMv: number }>, maxTicks = 6) {
  const tickLimit = Math.max(2, Math.floor(maxTicks))
  const uniqueIndices: number[] = []
  const seenVoltages = new Set<number>()

  points.forEach((point, index) => {
    if (seenVoltages.has(point.requestVoltageMv)) return
    seenVoltages.add(point.requestVoltageMv)
    uniqueIndices.push(index)
  })

  if (uniqueIndices.length <= tickLimit) return uniqueIndices

  const lastIndex = uniqueIndices.length - 1
  return Array.from({ length: tickLimit }, (_, tick) => {
    const position = Math.round((tick * lastIndex) / (tickLimit - 1))
    return uniqueIndices[position] as number
  })
}

function formatTemp(centiC: number | undefined) {
  return ((centiC ?? 0) / 100).toFixed(1)
}

function formatBoundaryVoltage(point: SupplyBoundary['points'][number]) {
  return formatBoundaryVoltageMv(point[6] || point[7])
}

function formatBoundaryVoltageMv(value: number) {
  return (value / 1000).toFixed(1)
}

function formatBoundaryCurrent(point: SupplyBoundary['points'][number]) {
  return (point[5] / 1000).toFixed(2)
}

function formatBoundaryCurrentMa(value: number) {
  return (value / 1000).toFixed(1)
}

function formatElapsed(elapsedMs: number) {
  const totalSeconds = Math.max(0, Math.round(elapsedMs / 1000))
  return `${Math.floor(totalSeconds / 60)}:${String(totalSeconds % 60).padStart(2, '0')}`
}

function formatOptional(value: number | null | undefined, fractionDigits: number) {
  return value == null ? '—' : value.toFixed(fractionDigits)
}

function formatThermalCapacity(value: number | null | undefined) {
  return value == null ? '—' : (value / 1000).toFixed(1)
}

function formatRadiation(value: number | null | undefined) {
  return value == null ? '—' : (value * 1000).toFixed(2)
}

function formatDelay(value: number | null | undefined) {
  return value == null ? '—' : (value / 1000).toFixed(2)
}
