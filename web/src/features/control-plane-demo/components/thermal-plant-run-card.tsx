import {
  Activity,
  CheckCircle2,
  CircleStop,
  Gauge,
  type LucideIcon,
  Microscope,
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
  const isRunning = attempt?.status === 'running'
  const showActiveResult = Boolean(snapshot.activeResult) && !isRunning
  const result = showActiveResult ? snapshot.activeResult : snapshot.provisionalCurve
  const curve =
    result?.curve.points.filter((point): point is NonNullable<typeof point> => point != null) ?? []
  const trace = snapshot.tracePage.points
  const status = unsupported ? 'unsupported' : (attempt?.status ?? 'idle')
  const canStart = !unsupported && !isRunning && (attempt == null || attempt.restartAllowed)

  return (
    <article className="thermal-plant-run-card" aria-label="自动热模型标定结果">
      <header className="thermal-plant-run-card__header">
        <div>
          <p className="thermal-plant-run-card__eyebrow">自动热模型标定</p>
          <p className="thermal-plant-run-card__meta">
            {attempt
              ? `运行 #${attempt.runId} · ${statusLabel[status] ?? status}${showActiveResult ? ' · EEPROM 已提交' : attempt.error ? ` · ${attempt.error}` : ''}`
              : '尚未运行'}
          </p>
        </div>
        <div className="thermal-plant-run-card__header-actions">
          <span
            className={`thermal-plant-run-card__status thermal-plant-run-card__status--${status}`}
          >
            <CheckCircle2 size={14} aria-hidden="true" />
            {unsupported ? '兼容状态' : (statusLabel[status] ?? status)}
          </span>
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
              <Activity size={16} aria-hidden="true" />
            )}
            {isRunning ? '停止自动校准' : '开始自动校准'}
          </button>
        </div>
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
        <section className="thermal-plant-run-card__chart" aria-label="温度轨迹">
          <div className="thermal-plant-run-card__section-heading">
            <div>
              <h3>温度轨迹</h3>
            </div>
            <span>{snapshot.tracePage.totalSamples} 点</span>
          </div>
          <TraceChart points={trace} />
          <div className="thermal-plant-run-card__legend">
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

        <section
          className="thermal-plant-run-card__chart thermal-plant-run-card__chart--curve"
          aria-label="电阻曲线"
        >
          <div className="thermal-plant-run-card__section-heading">
            <div>
              <h3>R(T) 加热曲线</h3>
            </div>
            <span>{curve.length} 点</span>
          </div>
          <ResistanceChart points={curve} />
        </section>

        <section className="thermal-plant-run-card__table-wrap" aria-label="代表点记录">
          <div className="thermal-plant-run-card__section-heading">
            <div>
              <h3>代表点记录</h3>
            </div>
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
                  <td>{formatTemp(point.tempCentiC)}℃</td>
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
      </div>
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
    return <div className="thermal-plant-run-card__chart-empty">等待温度轨迹</div>
  const width = 520
  const height = 180
  const minT = Math.min(...points.map((point) => point.temperatureCentiC / 100), 0)
  const maxT = Math.max(...points.map((point) => point.temperatureCentiC / 100), 220)
  const maxElapsed = Math.max(...points.map((point) => point.elapsedMs), 1)
  const path = points
    .map(
      (point) =>
        `${(point.elapsedMs / maxElapsed) * width},${height - ((point.temperatureCentiC / 100 - minT) / Math.max(maxT - minT, 1)) * (height - 12)}`
    )
    .join(' ')
  return (
    <svg
      className="thermal-plant-run-card__svg"
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label="加热和自然冷却温度曲线"
    >
      <line
        x1="0"
        y1={height - 1}
        x2={width}
        y2={height - 1}
        className="thermal-plant-run-card__axis"
      />
      <polyline points={path} className="thermal-plant-run-card__trace" />
      {points.map((point) => (
        <circle
          key={point.sampleIndex}
          cx={(point.elapsedMs / maxElapsed) * width}
          cy={
            height -
            ((point.temperatureCentiC / 100 - minT) / Math.max(maxT - minT, 1)) * (height - 12)
          }
          r="3"
          className={`thermal-plant-run-card__point thermal-plant-run-card__point--${point.phase}`}
        />
      ))}
    </svg>
  )
}

function ResistanceChart({
  points,
}: {
  points: Array<{ tempCentiC: number; resistanceMilliohms: number }>
}) {
  if (points.length < 2)
    return <div className="thermal-plant-run-card__chart-empty">等待 R(T) 预览</div>
  const width = 520
  const height = 180
  const minX = Math.min(...points.map((point) => point.tempCentiC), 0)
  const maxX = Math.max(...points.map((point) => point.tempCentiC), 22000)
  const minY = Math.min(...points.map((point) => point.resistanceMilliohms), 0)
  const maxY = Math.max(...points.map((point) => point.resistanceMilliohms), 1)
  const path = points
    .map(
      (point) =>
        `${((point.tempCentiC - minX) / Math.max(maxX - minX, 1)) * width},${height - ((point.resistanceMilliohms - minY) / Math.max(maxY - minY, 1)) * (height - 12)}`
    )
    .join(' ')
  return (
    <svg
      className="thermal-plant-run-card__svg"
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label="电阻温度曲线"
    >
      <line
        x1="0"
        y1={height - 1}
        x2={width}
        y2={height - 1}
        className="thermal-plant-run-card__axis"
      />
      <polyline points={path} className="thermal-plant-run-card__curve" />
      {points.map((point) => (
        <circle
          key={point.tempCentiC}
          cx={((point.tempCentiC - minX) / Math.max(maxX - minX, 1)) * width}
          cy={
            height - ((point.resistanceMilliohms - minY) / Math.max(maxY - minY, 1)) * (height - 12)
          }
          r="3"
          className="thermal-plant-run-card__point thermal-plant-run-card__point--curve"
        />
      ))}
    </svg>
  )
}

function formatTemp(centiC: number | undefined) {
  return ((centiC ?? 0) / 100).toFixed(1)
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
