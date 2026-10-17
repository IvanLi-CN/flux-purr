import type { ThermalTuningCommand, ThermalTuningEvent, ThermalTuningStatus } from './contracts'

export interface ThermalTuningRecording {
  runId: string | null
  status: ThermalTuningStatus
  events: ThermalTuningEvent[]
  acknowledgedThrough: number | null
  confirmedThrough: number | null
  gap: boolean
  gapReason: string | null
  complete: boolean
}

export function nextTuningReadCommand(
  recording: ThermalTuningRecording | null,
  identityVerified: boolean
): Omit<ThermalTuningCommand, 'leaseId'> | null {
  if (!identityVerified || !recording?.runId || recording.acknowledgedThrough == null) return null
  return { op: 'read', runId: recording.runId, afterSequence: recording.acknowledgedThrough }
}

export function recordThermalTuningPage(
  previous: ThermalTuningRecording | null,
  status: ThermalTuningStatus,
  confirmedSequence: number | null = null
): ThermalTuningRecording {
  const sameRun = previous?.runId === status.runId
  const events = sameRun ? [...previous.events] : []
  let gap = (sameRun && previous.gap) || status.trace.gap
  let gapReason = sameRun ? previous.gapReason : null
  if (status.trace.gap) gapReason ??= 'device_trace_gap'

  for (const event of status.events.events) {
    const existing = events[event.sequence]
    if (existing) {
      if (JSON.stringify(existing) !== JSON.stringify(event)) {
        gap = true
        gapReason ??= 'conflicting_event_sequence'
      }
      continue
    }
    const expected = events.length === 0 ? 0 : events[events.length - 1].sequence + 1
    if (event.sequence !== expected) {
      gap = true
      gapReason ??= 'missing_event_sequence'
      break
    }
    events.push(event)
  }

  const acknowledgedThrough = gap ? null : (events.at(-1)?.sequence ?? null)
  const confirmedThrough =
    !gap &&
    sameRun &&
    confirmedSequence != null &&
    previous.acknowledgedThrough != null &&
    confirmedSequence <= previous.acknowledgedThrough
      ? Math.max(previous.confirmedThrough ?? -1, confirmedSequence)
      : sameRun
        ? previous.confirmedThrough
        : null
  const complete =
    !gap &&
    status.trace.complete &&
    status.trace.firstSequence === 0 &&
    acknowledgedThrough === status.trace.lastSequence &&
    confirmedThrough === status.trace.lastSequence &&
    (status.events.nextSequence == null || status.events.nextSequence === acknowledgedThrough)
  return {
    runId: status.runId,
    status,
    events,
    acknowledgedThrough,
    confirmedThrough,
    gap,
    gapReason,
    complete,
  }
}
