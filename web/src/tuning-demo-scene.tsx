import { useRef, useState } from 'react'
import { FirmwareTuningWorkspace } from '@/features/control-plane-demo/components/firmware-tuning-workspace'
import type {
  ThermalTuningPhase,
  ThermalTuningStatus,
} from '@/features/control-plane-demo/contracts'
import { ControlPlaneClientError } from '@/features/control-plane-demo/transport-client'
import { tuningOwnerSession } from '@/features/control-plane-demo/tuning-owner-session'

const ownerProof = 'a'.repeat(64)
const EXECUTION_ORDER = [60, 220, 120, 80, 100, 160, 140, 180]
/** Stopped after three targets: the remaining five have no trace at all. */
const PARTIAL_TARGETS = EXECUTION_ORDER.slice(0, 3)
const powerClass = 'pps3a' as const

export const tuningDemoScenes = [
  'tuning-idle',
  'tuning-active',
  'tuning-save',
  'tuning-partial',
  'tuning-unsupported',
] as const

export type TuningDemoSceneId = (typeof tuningDemoScenes)[number]

export function isTuningDemoScene(scene: string | undefined): scene is TuningDemoSceneId {
  return typeof scene === 'string' && tuningDemoScenes.includes(scene as TuningDemoSceneId)
}

type PlotPhase = 'warmup' | 'approach' | 'hold'

/** One target's curve as ratios of its own target temperature. */
const activeShape: Array<{ phase: PlotPhase; ratio: number }> = [
  { phase: 'warmup', ratio: 0.753 },
  { phase: 'warmup', ratio: 0.835 },
  { phase: 'warmup', ratio: 0.927 },
  { phase: 'warmup', ratio: 0.973 },
  { phase: 'approach', ratio: 0.992 },
  { phase: 'approach', ratio: 1.002 },
  { phase: 'hold', ratio: 0.998 },
  { phase: 'hold', ratio: 1.003 },
  { phase: 'hold', ratio: 1.0 },
  { phase: 'hold', ratio: 1.002 },
  { phase: 'hold', ratio: 0.998 },
  { phase: 'hold', ratio: 1.0 },
]

/** Growth budget: one new sample every other poll, so a curve visibly fills in. */
const ACTIVE_SAMPLE_INTERVAL_READS = 2
const ACTIVE_INITIAL_SAMPLES = 3

type SavePhase = 'review' | 'sealed' | 'saved' | 'canceled'
type RunPhase = 'idle' | 'active' | 'canceled'

export function TuningDemoScene({ scene }: { scene: TuningDemoSceneId }) {
  const [deviceId] = useState(() => `demo-tuning-${crypto.randomUUID()}`)
  // The scene change remounts this component, so mount-time values are enough.
  // They live in a mutable box rather than state because a command and the read
  // that follows it run inside one await chain: state written by sendCommand is
  // still stale when that same chain immediately calls readStatus.
  const phase = useRef({
    savePhase: 'review' as SavePhase,
    runPhase: (scene === 'tuning-active' ? 'active' : 'idle') as RunPhase,
    activeReads: 0,
  }).current
  const identity = {
    runId: deviceId,
    candidateId: deviceId,
    candidateHash: `sha256:${'b'.repeat(64)}`,
    powerClass,
  }

  function traceFor(lastSequence: number, complete: boolean) {
    return {
      firstSequence: 0,
      lastSequence,
      complete,
      gap: false,
      digest: identity.candidateHash,
    }
  }

  function sampleEvent(
    sequence: number,
    plotPhase: ThermalTuningPhase,
    temperatureCentiC: number,
    targetC: number
  ): ThermalTuningStatus['events']['events'][number] {
    return {
      sequence,
      elapsedMs: sequence * 25_000,
      kind: 'sample',
      phase: plotPhase,
      targetC,
      trialIndex: 0,
      reason: null,
      trial: null,
      decision: null,
      sample: {
        ...identity,
        sequence,
        elapsedMs: sequence * 25_000,
        targetC,
        trialIndex: 0,
        phase: plotPhase,
        temperatureCentiC,
        measurementValid: true,
        vinMv: 18_000,
        requestedMv: 18_000,
        ppsPowerCeilingMv: 21_000,
        ppsContract: { voltageMv: 18_000, currentMa: 3_000, kind: powerClass, confirmed: true },
        heaterRequestedOutputPermille: 250,
        heaterOutputPermille: 250,
      },
    }
  }

  /** Standby: a resolved power class, but no run, no targets and no trace yet. */
  function idleStatus(): ThermalTuningStatus {
    const trace = traceFor(-1, true)
    return {
      phase: 'idle',
      trialPurpose: 'scout',
      terminalDisposition: 'running',
      reviewDisposition: 'pending',
      powerClass,
      runId: null,
      candidateId: null,
      candidateHash: null,
      targetsC: [],
      targetIndex: 0,
      armed: false,
      sealed: false,
      saveConfirmed: false,
      candidateProfile: null,
      previewReceipt: null,
      trace,
      events: { trace, events: [], nextSequence: null },
      saveReceipt: null,
    }
  }

  function canceledStatus(): ThermalTuningStatus {
    const { events } = activeRun(EXECUTION_ORDER[0], activeShape.length)
    const stopped = events.slice(0, 6)
    const trace = traceFor(stopped.length - 1, true)
    return {
      ...identity,
      phase: 'canceled',
      trialPurpose: 'scout',
      terminalDisposition: 'canceled',
      reviewDisposition: 'complete',
      targetsC: [EXECUTION_ORDER[0]],
      targetIndex: 0,
      armed: false,
      sealed: false,
      saveConfirmed: false,
      candidateProfile: null,
      previewReceipt: null,
      trace,
      events: { trace, events: stopped, nextSequence: trace.lastSequence },
      saveReceipt: null,
    }
  }

  /** `count` samples over the targets, spending one target's slots before the next. */
  function activeRun(startTargetIndex: number, count: number) {
    const events: ThermalTuningStatus['events']['events'] = []
    for (let index = 0; index < count; index += 1) {
      const targetIndex = startTargetIndex + Math.floor(index / activeShape.length)
      const targetC = EXECUTION_ORDER[Math.min(targetIndex, EXECUTION_ORDER.length - 1)]
      const { phase, ratio } = activeShape[index % activeShape.length]
      events.push(sampleEvent(index, phase, Math.round(targetC * ratio * 100), targetC))
    }
    return { events }
  }

  /** Running: every other read reveals one more sample, walking the targets. */
  function activeStatus(after = -1): ThermalTuningStatus {
    const length = activeShape.length * EXECUTION_ORDER.length
    const visible = Math.min(
      length,
      ACTIVE_INITIAL_SAMPLES + Math.floor(phase.activeReads / ACTIVE_SAMPLE_INTERVAL_READS)
    )
    const { events } = activeRun(0, visible)
    const trace = traceFor(visible - 1, visible >= length)
    const reached = events.slice(-1)[0]?.targetC ?? EXECUTION_ORDER[0]
    return {
      ...identity,
      phase: events[visible - 1].phase,
      trialPurpose: 'scout',
      terminalDisposition: 'running',
      reviewDisposition: 'pending',
      targetsC: EXECUTION_ORDER.slice(0, EXECUTION_ORDER.indexOf(reached) + 1),
      targetIndex: EXECUTION_ORDER.indexOf(reached),
      armed: true,
      sealed: false,
      saveConfirmed: false,
      candidateProfile: null,
      previewReceipt: null,
      trace,
      events: {
        trace,
        events: events.filter(({ sequence }) => sequence > after),
        nextSequence: visible >= length ? trace.lastSequence : null,
      },
      saveReceipt: null,
    }
  }

  /** A run over `targets`: only those targets carry samples. */
  function saveStatus(
    current: SavePhase,
    after = -1,
    targets: readonly number[] = EXECUTION_ORDER
  ): ThermalTuningStatus {
    const events: ThermalTuningStatus['events']['events'] = []
    const pushSample = (
      targetC: number,
      phase: 'warmup' | 'approach' | 'hold',
      temperatureCentiC: number,
      sequence: number
    ) => {
      events.push({
        sequence,
        elapsedMs: sequence * 25_000,
        kind: 'sample',
        phase,
        targetC,
        trialIndex: 0,
        reason: null,
        trial: null,
        decision: null,
        sample: {
          ...identity,
          sequence,
          elapsedMs: sequence * 25_000,
          targetC,
          trialIndex: 0,
          phase,
          temperatureCentiC,
          measurementValid: true,
          vinMv: 18_000,
          requestedMv: 18_000,
          ppsPowerCeilingMv: 21_000,
          ppsContract: { voltageMv: 18_000, currentMa: 3_000, kind: powerClass, confirmed: true },
          heaterRequestedOutputPermille: 250,
          heaterOutputPermille: 250,
        },
      })
    }
    targets.forEach((targetC, targetIndex) => {
      pushSample(targetC, 'warmup', Math.round(targetC * 0.72 * 100), events.length)
      pushSample(targetC, 'approach', Math.round(targetC * 0.995 * 100), events.length)
      pushSample(targetC, 'hold', Math.round(targetC * 1.002 * 100), events.length)
      events.push({
        sequence: events.length,
        elapsedMs: events.length * 25_000,
        kind: 'decision',
        phase: 'hold',
        targetC,
        trialIndex: 0,
        sample: null,
        reason: null,
        trial: null,
        decision: {
          disposition: 'adopted',
          purpose: 'hold_confirm',
          holdPeakToPeakCentiC: 34 + targetIndex * 7,
        },
      })
    })
    const saved = current === 'saved'
    const sealed = current === 'sealed'
    const canceled = current === 'canceled'
    const tail = events.at(-1) as ThermalTuningStatus['events']['events'][number]
    if (sealed || saved)
      events.push({
        ...tail,
        sequence: events.length,
        kind: 'phase_transition',
        phase: 'sealed',
        decision: null,
      })
    if (saved)
      events.push({
        ...tail,
        sequence: events.length,
        kind: 'phase_transition',
        phase: 'completed',
        decision: null,
      })
    const lastTrace = traceFor(events.length - 1, true)
    return {
      ...identity,
      phase: canceled ? 'canceled' : sealed ? 'sealed' : 'completed',
      terminalDisposition: saved ? 'passed' : canceled ? 'canceled' : 'candidate_ready',
      trialPurpose: 'hold_confirm',
      reviewDisposition: 'complete',
      targetsC: [...targets],
      targetIndex: targets.length - 1,
      armed: false,
      sealed,
      saveConfirmed: saved,
      candidateProfile: null,
      previewReceipt: {},
      trace: lastTrace,
      events: {
        trace: lastTrace,
        nextSequence: null,
        // A read only carries what follows its cursor; re-sending acknowledged
        // events would keep confirmedThrough one sequence behind the tail and
        // the save path would exhaust its acknowledgement budget.
        events: events.filter(({ sequence }) => sequence > after),
      },
      saveReceipt: saved
        ? {
            ...identity,
            bank: powerClass,
            bankRevision: 213,
            bankHash: identity.candidateHash,
            atomic: true,
            readbackVerified: true,
          }
        : null,
    }
  }

  if (scene === 'tuning-unsupported') {
    return (
      <FirmwareTuningWorkspace
        supported={false}
        deviceId={deviceId}
        transport="mock"
        readStatus={async () => {
          throw new ControlPlaneClientError(
            '当前设备固件不支持固件热控调优。',
            'thermal_tuning_unsupported',
            false
          )
        }}
        sendCommand={async () => {
          throw new Error('当前设备固件不支持固件热控调优。')
        }}
      />
    )
  }

  if (scene === 'tuning-active') {
    return (
      <FirmwareTuningWorkspace
        supported
        deviceId={deviceId}
        transport="mock"
        readStatus={async (cursor) => {
          phase.activeReads += 1
          return activeStatus(cursor?.afterSequence)
        }}
        sendCommand={async (command) => {
          if (command.op !== 'cancel') throw new Error('本轮调优已在进行中。')
          phase.runPhase = 'canceled'
          return canceledStatus()
        }}
      />
    )
  }

  return (
    <FirmwareTuningWorkspace
      supported
      deviceId={deviceId}
      transport="mock"
      readStatus={async (cursor) => {
        if (scene === 'tuning-idle') {
          if (phase.runPhase === 'active') {
            phase.activeReads += 1
            return activeStatus(cursor?.afterSequence)
          }
          return phase.runPhase === 'canceled' ? canceledStatus() : idleStatus()
        }
        if (phase.savePhase === 'saved')
          throw new ControlPlaneClientError(
            'Mock response exceeded the LAN envelope.',
            'response_too_large',
            false
          )
        const next = saveStatus(
          phase.savePhase,
          cursor?.afterSequence,
          scene === 'tuning-partial' ? PARTIAL_TARGETS : EXECUTION_ORDER
        )
        if (phase.savePhase === 'review' && !tuningOwnerSession.proofFor(deviceId, deviceId)) {
          tuningOwnerSession.acceptResponse(deviceId, 'start', { ...next, ownerProof })
        }
        return next
      }}
      sendCommand={async (command) => {
        if (scene === 'tuning-idle') {
          if (command.op === 'start') {
            phase.runPhase = 'active'
            phase.activeReads = 0
            // The start response carries the ownership proof for this tab; the
            // workspace registers it against the run identity it actually got.
            return { ...activeStatus(), ownerProof }
          }
          if (command.op === 'cancel') {
            phase.runPhase = 'canceled'
            return canceledStatus()
          }
          throw new Error('待命场景只接受开始或停止。')
        }
        const targets = scene === 'tuning-partial' ? PARTIAL_TARGETS : EXECUTION_ORDER
        if (command.op === 'seal' && phase.savePhase === 'review') {
          phase.savePhase = 'sealed'
          return saveStatus('sealed', -1, targets)
        }
        if (
          command.op === 'confirm_save' &&
          phase.savePhase === 'sealed' &&
          command.ownerProof === ownerProof
        ) {
          phase.savePhase = 'saved'
          return saveStatus('saved', -1, targets)
        }
        if (command.op === 'cancel' && phase.savePhase !== 'saved') {
          phase.savePhase = 'canceled'
          return saveStatus('canceled', -1, targets)
        }
        throw new Error('请在 Demo Inspector 中选「重置应用状态」后重试。')
      }}
    />
  )
}
