import {
  Activity,
  CircleStop,
  Gauge,
  LoaderCircle,
  type LucideIcon,
  Microscope,
  RefreshCw,
  Thermometer,
  TimerReset,
  Zap,
} from 'lucide-react'
import { useState } from 'react'
import { ScrollArea } from '@/components/ui/scroll-area'
import type { SupplyBoundary, ThermalPlantRunSnapshot, ThermalPlantTracePoint } from '../contracts'
import { thermalChartStyle } from './thermal-chart-style'
import { SupplyObservationTable, TemperatureRequestChart } from './thermal-supply-observations'
import { useResponsiveSvgHeight } from './use-responsive-svg-height'

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
  unsupported = false,
  loading = false,
  readError?: string
) {
  const attempt = snapshot.attempt
  const isRunning = attempt?.status === 'running'
  const status = readError
    ? 'error'
    : loading
      ? 'loading'
      : unsupported
        ? 'unsupported'
        : (attempt?.status ?? 'idle')
  const terminalFailure = status === 'failed' || status === 'canceled'
  const showActiveResult = Boolean(snapshot.activeResult) && !isRunning
  const preservesPriorActive = showActiveResult && terminalFailure
  const trace = snapshot.tracePage.points
  const statusText = readError
    ? '读取失败'
    : loading
      ? '读取中'
      : unsupported
        ? '兼容状态'
        : terminalFailure
          ? (statusLabel[status] ?? status)
          : showActiveResult
            ? 'active 有效'
            : (statusLabel[status] ?? status)
  const runEvidence = readError
    ? `设备快照读取失败 · ${readError}`
    : loading
      ? '正在读取设备快照 · 数据到达后显示'
      : isRunning
        ? `${phaseLabel[attempt?.phase ?? 'ambient']}中 · ${attempt?.sampleCount ?? trace.length} / ${Math.max(snapshot.tracePage.totalSamples, attempt?.sampleCount ?? 0)} 样本`
        : terminalFailure
          ? `${statusLabel[status]} · 本次未激活${attempt?.error ? ` · ${attempt.error}` : ''}${preservesPriorActive ? ' · 当前 active 保留' : ''}`
          : showActiveResult
            ? `${trace.length} / ${snapshot.tracePage.totalSamples} 瞬态样本 · EEPROM 中的有效结果`
            : '设备已就绪 · 尚未开始自动热模型标定'
  const traceStatus = readError
    ? '读取失败'
    : loading
      ? '读取中'
      : isRunning
        ? '采样中'
        : attempt?.status === 'completed'
          ? '80℃完成'
          : terminalFailure
            ? (statusLabel[status] ?? status)
            : trace.length > 0
              ? '已记录'
              : '待开始'

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
  loading = false,
  readError,
  runtimeRunning,
  onStartStop,
}: {
  snapshot: ThermalPlantRunSnapshot
  disabled?: boolean
  unsupported?: boolean
  loading?: boolean
  readError?: string
  runtimeRunning?: boolean
  onStartStop: () => void
}) {
  const attempt = snapshot.attempt
  const presentation = thermalPlantRunCardPresentation(snapshot, unsupported, loading, readError)
  const { showActiveResult } = presentation
  const isRunning = runtimeRunning ?? presentation.isRunning
  const boundary = snapshot.supplyBoundary
  const boundaryPoints = boundary?.points ?? []
  const selectionKey = `${attempt?.runId ?? 'active'}:${snapshot.supplyBoundarySource ?? 'unknown'}`
  const [selection, setSelection] = useState({ key: selectionKey, index: 0 })
  const selected =
    selection.key === selectionKey && selection.index < boundaryPoints.length ? selection.index : 0
  const setSelectedIndex = (index: number) => setSelection({ key: selectionKey, index })
  const trace = snapshot.tracePage.points
  const canControl = !unsupported && (isRunning || attempt == null || attempt.restartAllowed)
  const hasNoRunData =
    !loading &&
    !readError &&
    !unsupported &&
    !snapshot.activeResult &&
    !attempt &&
    trace.length === 0 &&
    boundaryPoints.length === 0

  return (
    <article
      className={`thermal-plant-run-card${loading ? ' thermal-plant-run-card--loading' : ''}`}
      aria-busy={loading}
      aria-label="自动热模型标定结果"
    >
      {!loading ? (
        <header
          className={`thermal-plant-run-card__header${readError || hasNoRunData ? ' thermal-plant-run-card__header--with-summary' : ''}`}
        >
          {readError ? (
            <StateSummary icon={Activity} title="快照读取失败" detail="请检查设备连接后重试" />
          ) : hasNoRunData ? (
            <StateSummary icon={Activity} title="设备已就绪" detail="尚未开始自动标定" />
          ) : null}
          <output
            className={`thermal-plant-run-card__status thermal-plant-run-card__status--${presentation.status}`}
          >
            <span aria-hidden="true" />
            {presentation.statusText}
          </output>
        </header>
      ) : null}

      {unsupported ? (
        <p className="thermal-plant-run-card__compatibility">
          当前固件未提供运行快照，保留通用自动校准状态读取。
        </p>
      ) : null}

      <div className="thermal-plant-run-card__metrics">
        {!loading && showActiveResult && snapshot.activeResult ? (
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
              value={attempt ? phaseLabel[attempt.phase ?? 'ambient'] : '待开始'}
              loading={loading}
            />
            <Metric
              icon={Activity}
              label="进度"
              value={attempt ? `${attempt.progressPercent}%` : '—'}
              loading={loading}
            />
            <Metric
              icon={Zap}
              label="当前温度"
              value={attempt ? `${formatTemp(attempt.currentTempCentiC)}℃` : '—'}
              loading={loading}
            />
            <Metric
              icon={Gauge}
              label="采样"
              value={attempt ? `${attempt.sampleCount} 点` : '—'}
              loading={loading}
            />
          </>
        )}
      </div>

      <div className="thermal-plant-run-card__body">
        <div className="thermal-plant-run-card__charts">
          <section
            className="thermal-plant-run-card__chart thermal-plant-run-card__chart--boundary"
            aria-label="温度—申请电压"
          >
            <div className="thermal-plant-run-card__section-heading">
              <h3>温度—申请电压</h3>
              <span>{loading ? '读取中' : `${boundaryPoints.length} 点`}</span>
            </div>
            <TemperatureRequestChart
              boundary={boundary}
              loading={loading}
              running={isRunning}
              selectedIndex={selected}
              onSelect={setSelectedIndex}
            />
          </section>

          <section className="thermal-plant-run-card__trace-panel" aria-label="温度轨迹">
            <div className="thermal-plant-run-card__section-heading">
              <h3>温度轨迹</h3>
              <span>{presentation.traceStatus}</span>
            </div>
            <TraceChart points={trace} loading={loading} running={isRunning} />
            <div className="thermal-chart__footer">
              <ScrollArea className="thermal-chart__detail-scroll">
                <output className="thermal-chart__detail">
                  {loading
                    ? '读取中'
                    : trace.length
                      ? `温度 ${(trace[trace.length - 1].temperatureCentiC / 100).toFixed(1)}℃ · 时间 ${formatElapsed(trace[trace.length - 1].elapsedMs)} · ${phaseLabel[trace[trace.length - 1].phase]}`
                      : '等待温度采样'}
                </output>
              </ScrollArea>
              <span className="thermal-chart__axis-title">时间 / mm:ss</span>
            </div>
          </section>
        </div>

        <aside className="thermal-plant-run-card__evidence" aria-label="瞬态与供电边界证据">
          <SupplyObservationTable
            boundary={boundary}
            loading={loading}
            selectedIndex={selected}
            onSelect={setSelectedIndex}
            source={snapshot.supplyBoundarySource}
          />

          <footer className="thermal-plant-run-card__footer">
            {attempt?.failureContext ? (
              <p className="thermal-plant-run-card__failure-context">
                失败阶段：
                {
                  {
                    ambient: '环境',
                    heating: '加热',
                    cooling: '自然冷却',
                    fitting: '拟合',
                    persistence: '保存',
                  }[attempt.failureContext.stage]
                }{' '}
                · 总耗时 {(attempt.failureContext.elapsedMs / 1000).toFixed(1)} s · 阶段耗时{' '}
                {(attempt.failureContext.stageElapsedMs / 1000).toFixed(1)} s<br />
                未满足：
                {(
                  {
                    rtd_sensor_healthy: 'RTD 传感器读数正常',
                    passive_fan_off: '无安全风扇介入',
                    eeprom_write_and_readback_verified: 'EEPROM 写入与读回校验成功',
                    physical_fit_and_complete_trace: '完整轨迹与物理拟合通过',
                    valid_ambient_samples: '有效环境温度采样',
                    live_temperature_at_least_220c: '实时温度达到 220℃',
                    passive_temperature_at_most_80c: '自然冷却至 80℃',
                    total_elapsed_less_than_660000ms: '全流程在 11 分钟内保存成功',
                  } as Record<string, string>
                )[attempt.failureContext.condition] ?? attempt.failureContext.condition}
              </p>
            ) : null}
            <output
              className={`thermal-plant-run-card__run-evidence${loading ? ' thermal-plant-run-card__run-evidence--loading' : ''}`}
              aria-live="polite"
            >
              {loading ? (
                <LoaderCircle
                  className="thermal-plant-run-card__spinner"
                  size={15}
                  aria-hidden="true"
                />
              ) : (
                <Activity size={15} aria-hidden="true" />
              )}
              {presentation.runEvidence}
            </output>
            <button
              type="button"
              className={[
                'thermal-plant-run-card__command',
                isRunning ? 'thermal-plant-run-card__command--stop' : '',
                loading && !isRunning ? 'thermal-plant-run-card__command--loading' : '',
              ]
                .filter(Boolean)
                .join(' ')}
              disabled={disabled || (loading && !isRunning) || !canControl}
              onClick={onStartStop}
            >
              {isRunning ? (
                <CircleStop size={16} aria-hidden="true" />
              ) : loading ? (
                <LoaderCircle
                  className="thermal-plant-run-card__spinner"
                  size={16}
                  aria-hidden="true"
                />
              ) : (
                <RefreshCw size={16} aria-hidden="true" />
              )}
              {isRunning
                ? '停止自动校准'
                : loading
                  ? '正在读取设备状态…'
                  : attempt == null
                    ? '开始自动校准'
                    : '重新开始自动校准'}
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
    supplyBoundarySource: 'attempt',
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

function Metric({
  icon: Icon,
  label,
  value,
  loading = false,
}: {
  icon: LucideIcon
  label: string
  value: string
  loading?: boolean
}) {
  return (
    <div
      className={`thermal-plant-run-card__metric${loading ? ' thermal-plant-run-card__metric--loading' : ''}`}
    >
      <Icon size={16} aria-hidden="true" />
      <span>
        <small>{label}</small>
        {loading ? (
          <output className="thermal-plant-run-card__metric-value-skeleton" aria-label="读取中">
            <span aria-hidden="true" />
          </output>
        ) : (
          <strong>{value}</strong>
        )}
      </span>
    </div>
  )
}

function StateSummary({
  icon: Icon,
  title,
  detail,
}: {
  icon: LucideIcon
  title: string
  detail: string
}) {
  return (
    <div className="thermal-plant-run-card__state-summary">
      <Icon size={15} aria-hidden="true" />
      <span>
        <strong>{title}</strong>
        <small>{detail}</small>
      </span>
    </div>
  )
}

function TraceChart({
  points,
  loading,
  running,
}: {
  points: ThermalPlantTracePoint[]
  loading: boolean
  running: boolean
}) {
  const { width, initialHeight, inset } = thermalChartStyle
  const { containerRef, viewBoxHeight } = useResponsiveSvgHeight(width, initialHeight)
  if (points.length < 2)
    return (
      <ChartPlaceholder
        loading={loading}
        variant="trace"
        icon={Thermometer}
        title={running ? '等待温度采样' : '尚未开始采样'}
        detail={
          running ? '设备已开始标定，温度轨迹会随着采样到达' : '开始自动校准后，温度轨迹会实时出现'
        }
      />
    )
  const height = viewBoxHeight
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
        <text x={inset.left} y={18}>
          温度 / ℃
        </text>
        <text x={width - inset.right} y={18} textAnchor="end" className="thermal-chart__key">
          <tspan className="thermal-chart__key--ambient">环境 · </tspan>
          <tspan className="thermal-chart__key--heating">加热 · </tspan>
          <tspan className="thermal-chart__key--cooling">自然冷却</tspan>
        </text>
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
            r="3"
            className="thermal-plant-run-card__point thermal-plant-run-card__point--heating"
          />
        ) : null}
        {completionPoint ? (
          <circle
            cx={completionPoint.x}
            cy={completionPoint.y}
            r="3"
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
        <text x={inset.left} y={plotBottom + 20}>
          {formatElapsed(0)}
        </text>
        {cutoffPoint ? (
          <text x={cutoffPoint.x} y={plotBottom + 20} textAnchor="middle">
            {formatElapsed(points[cutoffIndex]?.elapsedMs ?? 0)}
          </text>
        ) : null}
        <text x={width - inset.right} y={plotBottom + 20} textAnchor="end">
          {formatElapsed(maxElapsed)}
        </text>
      </svg>
    </div>
  )
}

function ChartLoadingSkeleton({ variant }: { variant: 'boundary' | 'trace' }) {
  const width = variant === 'boundary' ? 600 : 440
  const initialHeight = variant === 'boundary' ? 340 : 152
  const { containerRef, viewBoxHeight } = useResponsiveSvgHeight(width, initialHeight)
  const height = viewBoxHeight
  const inset =
    variant === 'boundary'
      ? {
          top: Math.max(16, height * 0.047),
          right: 26,
          bottom: Math.max(36, height * 0.106),
          left: 38,
        }
      : {
          top: Math.max(12, height * 0.079),
          right: 18,
          // No tick labels are shown while loading; keep only the axis-to-legend gap.
          bottom: 6,
          left: 34,
        }
  const plotWidth = width - inset.left - inset.right
  const plotBottom = height - inset.bottom
  const plotHeight = plotBottom - inset.top
  const gridRows = [0.25, 0.5, 0.75]
  const gridColumns = [0.2, 0.4, 0.6, 0.8]
  const smoothTrace = [
    `M ${inset.left} ${plotBottom - plotHeight * 0.08}`,
    `C ${inset.left + plotWidth * 0.16} ${plotBottom - plotHeight * 0.08},`,
    `${inset.left + plotWidth * 0.25} ${plotBottom - plotHeight * 0.18},`,
    `${inset.left + plotWidth * 0.38} ${inset.top + plotHeight * 0.12}`,
    `C ${inset.left + plotWidth * 0.52} ${inset.top + plotHeight * 0.02},`,
    `${inset.left + plotWidth * 0.57} ${inset.top + plotHeight * 0.58},`,
    `${inset.left + plotWidth * 0.72} ${inset.top + plotHeight * 0.68}`,
    `C ${inset.left + plotWidth * 0.84} ${inset.top + plotHeight * 0.76},`,
    `${inset.left + plotWidth * 0.91} ${inset.top + plotHeight * 0.8},`,
    `${width - inset.right} ${inset.top + plotHeight * 0.82}`,
  ].join(' ')
  const boundaryCurve = [
    `M ${inset.left} ${inset.top + plotHeight * 0.16}`,
    `C ${inset.left + plotWidth * 0.2} ${inset.top + plotHeight * 0.2},`,
    `${inset.left + plotWidth * 0.42} ${inset.top + plotHeight * 0.32},`,
    `${inset.left + plotWidth * 0.58} ${inset.top + plotHeight * 0.46}`,
    `C ${inset.left + plotWidth * 0.74} ${inset.top + plotHeight * 0.6},`,
    `${inset.left + plotWidth * 0.88} ${inset.top + plotHeight * 0.72},`,
    `${width - inset.right} ${plotBottom - plotHeight * 0.06}`,
  ].join(' ')

  return (
    <div
      ref={containerRef}
      className={`thermal-plant-run-card__chart-skeleton thermal-plant-run-card__chart-skeleton--${variant}`}
      aria-hidden="true"
    >
      <svg
        className="thermal-plant-run-card__chart-skeleton-canvas"
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="xMidYMid meet"
        aria-hidden="true"
      >
        <g className="thermal-plant-run-card__chart-skeleton-grid">
          {gridRows.map((position) => (
            <line
              key={`row-${position}`}
              x1={inset.left}
              x2={width - inset.right}
              y1={inset.top + plotHeight * position}
              y2={inset.top + plotHeight * position}
            />
          ))}
          {gridColumns.map((position) => (
            <line
              key={`column-${position}`}
              x1={inset.left + plotWidth * position}
              x2={inset.left + plotWidth * position}
              y1={inset.top}
              y2={plotBottom}
            />
          ))}
        </g>
        <g className="thermal-plant-run-card__chart-skeleton-axes">
          <line x1={inset.left} x2={inset.left} y1={inset.top} y2={plotBottom} />
          <line x1={inset.left} x2={width - inset.right} y1={plotBottom} y2={plotBottom} />
        </g>
        <path
          className="thermal-plant-run-card__chart-skeleton-plot"
          d={variant === 'boundary' ? boundaryCurve : smoothTrace}
        />
      </svg>
      <div className="thermal-plant-run-card__chart-skeleton-status">
        <LoaderCircle size={15} aria-hidden="true" />
        <span>读取中</span>
      </div>
    </div>
  )
}

function ChartPlaceholder({
  loading,
  variant,
  icon: Icon,
  title,
  detail,
}: {
  loading: boolean
  variant: 'boundary' | 'trace'
  icon: LucideIcon
  title: string
  detail: string
}) {
  return (
    <div
      className={`thermal-plant-run-card__chart-empty thermal-plant-run-card__chart-empty--${variant}${loading ? ' thermal-plant-run-card__chart-empty--loading' : ''}`}
    >
      {loading ? (
        <ChartLoadingSkeleton variant={variant} />
      ) : (
        <>
          <div className="thermal-plant-run-card__placeholder-grid" aria-hidden="true">
            <span />
            <span />
            <span />
          </div>
          <div className="thermal-plant-run-card__placeholder-copy">
            <span className="thermal-plant-run-card__placeholder-icon" aria-hidden="true">
              <Icon size={17} />
            </span>
            <strong>{title}</strong>
            <span>{detail}</span>
          </div>
        </>
      )}
    </div>
  )
}

function formatTemp(centiC: number | undefined) {
  return ((centiC ?? 0) / 100).toFixed(1)
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
