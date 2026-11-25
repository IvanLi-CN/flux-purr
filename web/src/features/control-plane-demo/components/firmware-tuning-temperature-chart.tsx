import { ChartNoAxesCombined, Star } from 'lucide-react'
import { useCallback, useLayoutEffect, useMemo, useState } from 'react'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import type { ThermalTuningEvent } from '../contracts'

type Point = { time: number; temperature: number; segment: number }
const phaseLabels = { warmup: '预热', approach: '接近目标', hold: '稳温' } as const
type PlotPhase = keyof typeof phaseLabels
type Trial = { points: Point[]; phases: { phase: PlotPhase; time: number; sequence: number }[] }

/**
 * Keeps the tuning plot's viewBox at one unit per CSS pixel. The shared
 * height-only hook derives the viewBox height from a fixed viewBox width,
 * which rescales every axis label with the panel width.
 */
function usePlotViewBox() {
  const [container, setContainer] = useState<HTMLDivElement | null>(null)
  const [size, setSize] = useState({ width: 400, height: 106 })
  const containerRef = useCallback((node: HTMLDivElement | null) => setContainer(node), [])

  useLayoutEffect(() => {
    if (!container) return
    const update = () => {
      const rect = container.getBoundingClientRect()
      if (rect.width <= 0 || rect.height <= 0) return
      setSize((current) =>
        Math.abs(current.width - rect.width) < 0.5 && Math.abs(current.height - rect.height) < 0.5
          ? current
          : { width: rect.width, height: rect.height }
      )
    }
    let frameId = 0
    const scheduleUpdate = () => {
      update()
      if (typeof requestAnimationFrame !== 'function') return
      if (frameId !== 0) cancelAnimationFrame(frameId)
      frameId = requestAnimationFrame(() => {
        frameId = 0
        update()
      })
    }
    const observer =
      typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(scheduleUpdate)
    update()
    scheduleUpdate()
    observer?.observe(container)
    window.addEventListener('resize', scheduleUpdate)
    return () => {
      observer?.disconnect()
      if (frameId !== 0) cancelAnimationFrame(frameId)
      window.removeEventListener('resize', scheduleUpdate)
    }
  }, [container])

  return { containerRef, viewBoxWidth: size.width, viewBoxHeight: size.height }
}

function isPlotPhase(phase: string): phase is PlotPhase {
  return phase === 'warmup' || phase === 'approach' || phase === 'hold'
}

function TrialLabel({ index, best }: { index: number; best: boolean }) {
  return (
    <span className="firmware-tuning__trial-label">
      <span className="firmware-tuning__trial-text">第 {index + 1} 次</span>
      {best ? (
        <Star role="img" aria-label="最佳" className="firmware-tuning__trial-best size-3" />
      ) : null}
    </span>
  )
}

function collectTrials(events: readonly ThermalTuningEvent[]) {
  const targets = new Map<number, Map<number, Trial>>()
  const adoptedTrials = new Map<number, number>()
  const holdRanges = new Map<string, number>()
  let segment = 0
  let lastSequence: number | undefined
  for (const event of events) {
    if (lastSequence !== undefined && event.sequence !== lastSequence + 1) segment += 1
    lastSequence = event.sequence
    if (event.kind === 'decision' && event.targetC != null && event.trialIndex != null) {
      if (event.decision?.disposition === 'adopted') {
        adoptedTrials.set(event.targetC, event.trialIndex)
      }
      const range = event.decision?.holdPeakToPeakCentiC
      if (typeof range === 'number' && Number.isFinite(range)) {
        holdRanges.set(`${event.targetC}:${event.trialIndex}`, range / 100)
      }
    }
    const sample = event.sample
    if (!sample && event.kind !== 'phase_transition') continue
    const phase = sample?.phase ?? event.phase
    const targetC = sample?.targetC ?? event.targetC
    const trialIndex = sample?.trialIndex ?? event.trialIndex
    if (!isPlotPhase(phase) || targetC == null || trialIndex == null) continue
    let trials = targets.get(targetC)
    let trial = trials?.get(trialIndex)
    if (!trial && phase !== 'warmup') continue
    if (!trials) {
      trials = new Map()
      targets.set(targetC, trials)
    }
    if (!trial) {
      trial = { points: [], phases: [] }
      trials.set(trialIndex, trial)
    }
    const time = (sample?.elapsedMs ?? event.elapsedMs) / 1000
    if (trial.phases.at(-1)?.phase !== phase)
      trial.phases.push({ phase, time, sequence: event.sequence })
    if (!sample) continue
    if (!sample.measurementValid || !Number.isFinite(sample.temperatureCentiC)) {
      segment += 1
      continue
    }
    if (sample.measurementAccepted === false) continue
    trial.points.push({
      time,
      temperature: sample.temperatureCentiC / 100,
      segment,
    })
  }
  for (const [target, trials] of targets) {
    for (const [index, trial] of trials) {
      if (!trial.points.length) trials.delete(index)
    }
    if (!trials.size) targets.delete(target)
  }
  return { targets, adoptedTrials, holdRanges }
}

export function tuningChartTargets(events: readonly ThermalTuningEvent[]) {
  return [...collectTrials(events).targets.keys()].sort((left, right) => left - right)
}

export function FirmwareTuningTemperatureChart({
  events,
  selectedTarget,
  traceGap,
}: {
  events: readonly ThermalTuningEvent[]
  selectedTarget: number
  traceGap: boolean
}) {
  const { targets, adoptedTrials, holdRanges } = useMemo(() => collectTrials(events), [events])
  const [selectedTrial, setSelectedTrial] = useState<number>()
  const { containerRef, viewBoxWidth, viewBoxHeight } = usePlotViewBox()
  const target = selectedTarget
  const hasTargetData = targets.has(target)
  const trials = targets.get(target)
  const adoptedTrial = adoptedTrials.get(target)
  const trial =
    selectedTrial != null && trials?.has(selectedTrial)
      ? selectedTrial
      : [...(trials?.keys() ?? [])].at(-1)
  const series = trial != null ? trials?.get(trial) : undefined
  const points = series?.points
  if (!hasTargetData || trial == null || !points?.length) {
    return (
      <output className="firmware-tuning__chart-empty" aria-label="暂无有效温度采样">
        <ChartNoAxesCombined className="firmware-tuning__chart-empty-visual" aria-hidden="true" />
        <div className="firmware-tuning__chart-empty-copy">
          <strong>暂无有效温度采样</strong>
          <span>温度曲线将在有效采样到达后显示。</span>
        </div>
      </output>
    )
  }
  const trialOptions = [...(trials?.keys() ?? [])]
  const holdRange = holdRanges.get(`${target}:${trial}`)

  const phases = series?.phases ?? []
  const start = phases[0]?.time ?? points[0].time
  const end = Math.max(points[points.length - 1].time, phases.at(-1)?.time ?? start)
  const duration = end - start
  let low = target
  let peak = Number.NEGATIVE_INFINITY
  for (const point of points) {
    low = Math.min(low, point.temperature)
    peak = Math.max(peak, point.temperature)
  }
  const min = Math.floor((low - 1) / 2) * 2
  const max = Math.ceil((Math.max(target, peak) + 1) / 2) * 2
  const plotLeft = 42
  const plotRight = Math.max(plotLeft + 1, viewBoxWidth - 20)
  const plotTop = 30
  const x = (time: number) =>
    plotLeft + (plotRight - plotLeft) * ((time - start) / Math.max(1, duration))
  const plotBottom = viewBoxHeight - 26
  const y = (temperature: number) =>
    plotBottom - (plotBottom - plotTop) * ((temperature - min) / (max - min))
  const segments: Point[][] = []
  for (const point of points) {
    const last = segments.at(-1)
    if (last?.[0].segment === point.segment) last.push(point)
    else segments.push([point])
  }

  return (
    <div className="firmware-tuning__chart">
      <div className="firmware-tuning__chart-controls">
        <span className="firmware-tuning__chart-target-label">目标 {target} °C</span>
        <Select value={String(trial)} onValueChange={(value) => setSelectedTrial(Number(value))}>
          <SelectTrigger aria-label="曲线试验轮次" className="firmware-tuning__trial-select">
            <span className="firmware-tuning__trial-sizing">
              <SelectValue />
              {trialOptions.map((value, index) => (
                <span key={value} aria-hidden="true" className="firmware-tuning__trial-measure">
                  <TrialLabel index={index} best={value === adoptedTrial} />
                </span>
              ))}
            </span>
          </SelectTrigger>
          <SelectContent align="start">
            {trialOptions.map((value, index) => (
              <SelectItem key={value} value={String(value)}>
                <TrialLabel index={index} best={value === adoptedTrial} />
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <span>
          峰值 {peak.toFixed(1)} °C · 超调 {Math.max(0, peak - target).toFixed(1)} °C
        </span>
        <span>稳温峰峰值 {holdRange == null ? '未记录' : `${holdRange.toFixed(2)} °C`}</span>
      </div>
      <div className="firmware-tuning__chart-legend">
        {[...new Set(phases.map(({ phase }) => phase))].map((phase) => (
          <span key={phase}>
            <i aria-hidden="true" className={`firmware-tuning__chart-phase--${phase}`} />
            {phaseLabels[phase]}
          </span>
        ))}
      </div>
      <div className="firmware-tuning__chart-plot" ref={containerRef}>
        <svg
          viewBox={`0 0 ${viewBoxWidth} ${viewBoxHeight}`}
          role="img"
          aria-label={`${target} °C 温度曲线，虚线为目标温度`}
        >
          {phases.map(({ phase, time, sequence }, index) => (
            <rect
              key={sequence}
              className={`firmware-tuning__chart-phase--${phase}`}
              x={x(time)}
              y={plotTop}
              width={Math.max(0, x(phases[index + 1]?.time ?? end) - x(time))}
              height={Math.max(0, plotBottom - plotTop)}
            />
          ))}
          {[min, (min + max) / 2, max].map((tick) => (
            <g key={tick}>
              <text x={plotLeft - 8} y={y(tick) + 3} textAnchor="end">
                {tick}
              </text>
              <line
                x1={plotLeft}
                x2={plotRight}
                y1={y(tick)}
                y2={y(tick)}
                className="firmware-tuning__chart-grid"
              />
            </g>
          ))}
          <text x="4" y={plotTop - 14}>
            °C
          </text>
          <text x={plotLeft} y={viewBoxHeight - 4} textAnchor="middle">
            0
          </text>
          {duration > 0 ? (
            <text x={plotRight} y={viewBoxHeight - 4} textAnchor="end">
              {duration.toFixed(1)} s
            </text>
          ) : null}
          <line
            x1={plotLeft}
            x2={plotRight}
            y1={y(target)}
            y2={y(target)}
            className="firmware-tuning__chart-target"
          />
          {segments.map((part) =>
            part.length === 1 ? (
              <circle
                key={part[0].segment}
                cx={x(part[0].time)}
                cy={y(part[0].temperature)}
                r="2"
              />
            ) : (
              <polyline
                key={part[0].segment}
                points={part.map((p) => `${x(p.time)},${y(p.temperature)}`).join(' ')}
              />
            )
          )}
        </svg>
      </div>
      {traceGap ? <span className="firmware-tuning__chart-gap">采样记录不完整</span> : null}
    </div>
  )
}
