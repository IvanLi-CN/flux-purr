import { Star } from 'lucide-react'
import { RadioGroup } from 'radix-ui'
import { useMemo, useState } from 'react'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import type { ThermalTuningEvent } from '../contracts'
import { useResponsiveSvgHeight } from './use-responsive-svg-height'

type Point = { time: number; temperature: number; segment: number }
const phaseLabels = { warmup: '预热', approach: '接近目标', hold: '稳温' } as const
type PlotPhase = keyof typeof phaseLabels
type Trial = { points: Point[]; phases: { phase: PlotPhase; time: number; sequence: number }[] }

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

export function FirmwareTuningTemperatureChart({
  events,
  currentTarget,
  traceGap,
}: {
  events: readonly ThermalTuningEvent[]
  currentTarget?: number
  traceGap: boolean
}) {
  const { targets, adoptedTrials, holdRanges } = useMemo(() => collectTrials(events), [events])
  const [selectedTarget, setSelectedTarget] = useState<number>()
  const [selectedTrial, setSelectedTrial] = useState<number>()
  const { containerRef, viewBoxHeight } = useResponsiveSvgHeight(400, 106)
  const target =
    selectedTarget != null && targets.has(selectedTarget)
      ? selectedTarget
      : currentTarget != null && targets.has(currentTarget)
        ? currentTarget
        : [...targets.keys()].at(-1)
  const trials = target != null ? targets.get(target) : undefined
  const adoptedTrial = target != null ? adoptedTrials.get(target) : undefined
  const trial =
    selectedTrial != null && trials?.has(selectedTrial)
      ? selectedTrial
      : [...(trials?.keys() ?? [])].at(-1)
  const series = trial != null ? trials?.get(trial) : undefined
  const points = series?.points
  if (target == null || trial == null || !points?.length) {
    return <p className="firmware-tuning__chart-empty">暂无有效温度采样</p>
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
  const x = (time: number) => 42 + 338 * ((time - start) / Math.max(1, duration))
  const plotBottom = viewBoxHeight - 22
  const y = (temperature: number) =>
    plotBottom - (plotBottom - 14) * ((temperature - min) / (max - min))
  const segments: Point[][] = []
  for (const point of points) {
    const last = segments.at(-1)
    if (last?.[0].segment === point.segment) last.push(point)
    else segments.push([point])
  }

  return (
    <div className="firmware-tuning__chart">
      <div className="firmware-tuning__chart-controls">
        <RadioGroup.Root
          className="industrial-workspace-switch firmware-tuning__chart-targets"
          aria-label="曲线目标温度"
          value={String(target)}
          onValueChange={(value) => {
            setSelectedTarget(Number(value))
            setSelectedTrial(undefined)
          }}
        >
          {[...targets.keys()]
            .sort((a, b) => a - b)
            .map((value) => (
              <RadioGroup.Item
                key={value}
                value={String(value)}
                className={value === target ? 'is-selected' : undefined}
              >
                {value}°C
              </RadioGroup.Item>
            ))}
        </RadioGroup.Root>
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
          viewBox={`0 0 400 ${viewBoxHeight}`}
          role="img"
          aria-label={`${target} °C 温度曲线，虚线为目标温度`}
        >
          {phases.map(({ phase, time, sequence }, index) => (
            <rect
              key={sequence}
              className={`firmware-tuning__chart-phase--${phase}`}
              x={x(time)}
              y="14"
              width={Math.max(0, x(phases[index + 1]?.time ?? end) - x(time))}
              height={Math.max(0, plotBottom - 14)}
            />
          ))}
          {[min, (min + max) / 2, max].map((tick) => (
            <g key={tick}>
              <text x="34" y={y(tick) + 3} textAnchor="end">
                {tick}
              </text>
              <line
                x1="42"
                x2="380"
                y1={y(tick)}
                y2={y(tick)}
                className="firmware-tuning__chart-grid"
              />
            </g>
          ))}
          <text x="4" y="10">
            °C
          </text>
          <text x="42" y={viewBoxHeight - 4} textAnchor="middle">
            0
          </text>
          {duration > 0 ? (
            <text x="380" y={viewBoxHeight - 4} textAnchor="end">
              {duration.toFixed(1)} s
            </text>
          ) : null}
          <line
            x1="42"
            x2="380"
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
