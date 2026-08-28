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
import type { ThermalPlantRunSnapshot, ThermalPlantTracePoint } from '../contracts'

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
  const result = showActiveResult ? snapshot.activeResult : snapshot.provisionalCurve
  const curve =
    result?.curve.points.filter((point): point is NonNullable<typeof point> => point != null) ?? []
  const trace = snapshot.tracePage.points
  const canStart = !unsupported && !isRunning && (attempt == null || attempt.restartAllowed)

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
        <section
          className="thermal-plant-run-card__chart thermal-plant-run-card__chart--curve"
          aria-label="电阻曲线"
        >
          <div className="thermal-plant-run-card__section-heading">
            <h3>{presentation.preservesPriorActive ? 'R(T) 当前 active' : 'R(T) 加热曲线'}</h3>
            <span>{presentation.preservesPriorActive ? '上次成功' : `${curve.length} 点`}</span>
          </div>
          <ResistanceChart points={curve} />
        </section>

        <aside className="thermal-plant-run-card__evidence" aria-label="瞬态与代表点证据">
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

          <section className="thermal-plant-run-card__table-wrap" aria-label="代表点记录">
            <div className="thermal-plant-run-card__section-heading">
              <h3>R(T) 代表点</h3>
              <span>{curve.length} / 5</span>
            </div>
            <table>
              <thead>
                <tr>
                  <th>温度</th>
                  <th>R(T)</th>
                  <th>状态</th>
                </tr>
              </thead>
              <tbody>
                {curve.map((point) => (
                  <tr key={point.tempCentiC}>
                    <td>{formatChartTemp(point.tempCentiC)}℃</td>
                    <td>{(point.resistanceMilliohms / 1000).toFixed(3)} Ω</td>
                    <td>
                      <span className="thermal-plant-run-card__valid">有效</span>
                    </td>
                  </tr>
                ))}
                {curve.length === 0 ? (
                  <tr>
                    <td colSpan={3} className="thermal-plant-run-card__empty">
                      等待有效采样
                    </td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </section>
        </aside>
      </div>

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
          disabled={disabled || !canStart}
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
    </article>
  )
}

export function createDefaultThermalPlantSnapshot(): ThermalPlantRunSnapshot {
  const curve = [
    [25, 5674],
    [61, 6089],
    [102, 6583],
    [162, 7307],
    [220, 8011],
  ].map(([tempC, resistanceMilliohms]) => ({ tempCentiC: tempC * 100, resistanceMilliohms }))
  const temperatures = [25, 35, 52, 78, 112, 148, 182, 207, 220, 205, 174, 138, 102, 80]
  return {
    version: 1,
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
      curve: { points: [...curve, null, null, null] },
      convectionMwPerC: 120,
      radiationMwPerK4: 0.0002,
      thermalCapacityMjPerC: 42000,
      transportDelayMs: 500,
    },
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
  if (points.length < 2)
    return (
      <div className="thermal-plant-run-card__chart-empty thermal-plant-run-card__chart-empty--trace">
        等待温度轨迹
      </div>
    )
  const width = 440
  const height = 152
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
    <svg
      className="thermal-plant-run-card__svg thermal-plant-run-card__svg--trace"
      preserveAspectRatio="none"
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
  )
}

function ResistanceChart({
  points,
}: {
  points: Array<{ tempCentiC: number; resistanceMilliohms: number }>
}) {
  if (points.length < 2)
    return (
      <div className="thermal-plant-run-card__chart-empty thermal-plant-run-card__chart-empty--curve">
        等待 R(T) 预览
      </div>
    )
  const width = 600
  const height = 340
  const inset = {
    top: Math.max(16, height * 0.047),
    right: 26,
    bottom: Math.max(36, height * 0.106),
    left: 38,
  }
  const minX = Math.min(...points.map((point) => point.tempCentiC))
  const maxX = Math.max(...points.map((point) => point.tempCentiC))
  const rawMinY = Math.min(...points.map((point) => point.resistanceMilliohms))
  const rawMaxY = Math.max(...points.map((point) => point.resistanceMilliohms))
  const rangeY = Math.max(rawMaxY - rawMinY, 1)
  const minY = rawMinY - rangeY * 0.1
  const maxY = rawMaxY + rangeY * 0.1
  const plotWidth = width - inset.left - inset.right
  const plotBottom = height - inset.bottom
  const plotHeight = plotBottom - inset.top
  const positionFor = (point: { tempCentiC: number; resistanceMilliohms: number }) => ({
    x: inset.left + ((point.tempCentiC - minX) / Math.max(maxX - minX, 1)) * plotWidth,
    y: plotBottom - ((point.resistanceMilliohms - minY) / Math.max(maxY - minY, 1)) * plotHeight,
  })
  const positions = points.map(positionFor)
  const path = points
    .map((point) => {
      const position = positionFor(point)
      return `${position.x},${position.y}`
    })
    .join(' ')
  return (
    <svg
      className="thermal-plant-run-card__svg thermal-plant-run-card__svg--curve"
      preserveAspectRatio="none"
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label="电阻温度曲线"
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
      <polyline points={path} className="thermal-plant-run-card__curve" />
      {points.map((point, index) => (
        <circle
          key={point.tempCentiC}
          cx={positions[index]?.x}
          cy={positions[index]?.y}
          r="4"
          className="thermal-plant-run-card__point thermal-plant-run-card__point--curve"
        />
      ))}
      {points.map((point, index) => (
        <text
          key={`${point.tempCentiC}-label`}
          x={positions[index]?.x}
          y={height - 8}
          textAnchor="middle"
        >
          {formatChartTemp(point.tempCentiC)}℃
        </text>
      ))}
      <text x={inset.left - 16} y={inset.top + 5}>
        Ω
      </text>
    </svg>
  )
}

function formatTemp(centiC: number | undefined) {
  return ((centiC ?? 0) / 100).toFixed(1)
}

function formatChartTemp(centiC: number) {
  const temperature = centiC / 100
  return Number.isInteger(temperature) ? String(temperature) : temperature.toFixed(1)
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
