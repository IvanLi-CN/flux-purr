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

/** Deterministic in-progress curve: warmup, approach, then a trimmed hold. */
const activeCurve: Array<{ phase: PlotPhase; temperatureCentiC: number }> = [
  { phase: 'warmup', temperatureCentiC: 4_520 },
  { phase: 'warmup', temperatureCentiC: 5_010 },
  { phase: 'warmup', temperatureCentiC: 5_560 },
  { phase: 'warmup', temperatureCentiC: 5_840 },
  { phase: 'approach', temperatureCentiC: 5_950 },
  { phase: 'approach', temperatureCentiC: 6_010 },
  { phase: 'hold', temperatureCentiC: 5_990 },
  { phase: 'hold', temperatureCentiC: 6_020 },
  { phase: 'hold', temperatureCentiC: 6_000 },
  { phase: 'hold', temperatureCentiC: 6_010 },
  { phase: 'hold', temperatureCentiC: 5_990 },
  { phase: 'hold', temperatureCentiC: 6_000 },
]

type SavePhase = 'review' | 'sealed' | 'saved' | 'canceled'
type RunPhase = 'idle' | 'active' | 'canceled'

export function TuningDemoScene({ scene }: { scene: TuningDemoSceneId }) {
  const [deviceId] = useState(() => `demo-tuning-${crypto.randomUUID()}`)
  const savePhase = useRef<SavePhase>('review')
  const runPhase = useRef<RunPhase>(scene === 'tuning-active' ? 'active' : 'idle')
  const activeReads = useRef(0)
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
    phase: ThermalTuningPhase,
    temperatureCentiC: number
  ): ThermalTuningStatus['events']['events'][number] {
    return {
      sequence,
      elapsedMs: sequence * 25_000,
      kind: 'sample',
      phase,
      targetC: 60,
      trialIndex: 0,
      reason: null,
      trial: null,
      decision: null,
      sample: {
        ...identity,
        sequence,
        elapsedMs: sequence * 25_000,
        targetC: 60,
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
    }
  }

  /** Standby: the workbench has a power class and identity but no run yet. */
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
    const events = activeCurve
      .slice(0, 6)
      .map((point, sequence) => sampleEvent(sequence, point.phase, point.temperatureCentiC))
    const trace = traceFor(events.length - 1, true)
    return {
      ...identity,
      phase: 'canceled',
      trialPurpose: 'scout',
      terminalDisposition: 'canceled',
      reviewDisposition: 'complete',
      targetsC: EXECUTION_ORDER,
      targetIndex: 0,
      armed: false,
      sealed: false,
      saveConfirmed: false,
      candidateProfile: null,
      previewReceipt: null,
      trace,
      events: { trace, events, nextSequence: trace.lastSequence },
      saveReceipt: null,
    }
  }

  /** Running: every read reveals a few more deterministic samples. */
  function activeStatus(after = -1): ThermalTuningStatus {
    const visible = Math.min(activeCurve.length, 3 + activeReads.current * 2)
    const events = activeCurve
      .slice(0, visible)
      .map((point, sequence) => sampleEvent(sequence, point.phase, point.temperatureCentiC))
    const trace = traceFor(visible - 1, false)
    return {
      ...identity,
      phase: activeCurve[visible - 1].phase,
      trialPurpose: 'scout',
      terminalDisposition: 'running',
      reviewDisposition: 'pending',
      targetsC: EXECUTION_ORDER,
      targetIndex: 0,
      armed: true,
      sealed: false,
      saveConfirmed: false,
      candidateProfile: null,
      previewReceipt: null,
      trace,
      events: {
        trace,
        events: events.filter(({ sequence }) => sequence > after),
        nextSequence: null,
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
      const curve = [
        { phase: 'warmup' as const, ratio: 0.72 },
        { phase: 'warmup' as const, ratio: 0.94 },
        { phase: 'approach' as const, ratio: 0.99 },
        { phase: 'hold' as const, ratio: 1.004 },
        { phase: 'hold' as const, ratio: 1.001 },
      ]
      for (const { phase, ratio } of curve) {
        pushSample(targetC, phase, Math.round(targetC * ratio * 100), events.length)
      }
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
    const trace = traceFor(events.length - 1, true)
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
      trace,
      events: {
        trace,
        events: events.filter(({ sequence }) => sequence > after),
        nextSequence: trace.lastSequence,
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
          activeReads.current += 1
          return activeStatus(cursor?.afterSequence)
        }}
        sendCommand={async (command) => {
          if (command.op !== 'cancel') throw new Error('本轮调优已在进行中。')
          runPhase.current = 'canceled'
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
          if (runPhase.current === 'active') {
            activeReads.current += 1
            return activeStatus(cursor?.afterSequence)
          }
          return runPhase.current === 'canceled' ? canceledStatus() : idleStatus()
        }
        if (savePhase.current === 'saved')
          throw new ControlPlaneClientError(
            'Mock response exceeded the LAN envelope.',
            'response_too_large',
            false
          )
        const next = saveStatus(
          savePhase.current,
          cursor?.afterSequence,
          scene === 'tuning-partial' ? PARTIAL_TARGETS : EXECUTION_ORDER
        )
        if (savePhase.current === 'review' && !tuningOwnerSession.proofFor(deviceId, deviceId)) {
          tuningOwnerSession.acceptResponse(deviceId, 'start', { ...next, ownerProof })
        }
        return next
      }}
      sendCommand={async (command) => {
        if (scene === 'tuning-idle') {
          if (command.op === 'start') {
            runPhase.current = 'active'
            // The start response carries the ownership proof for this tab; the
            // workspace registers it against the run identity it actually got.
            return { ...activeStatus(), ownerProof }
          }
          if (command.op === 'cancel') {
            runPhase.current = 'canceled'
            return canceledStatus()
          }
          throw new Error('待命场景只接受开始或停止。')
        }
        const targets = scene === 'tuning-partial' ? PARTIAL_TARGETS : EXECUTION_ORDER
        if (command.op === 'seal' && savePhase.current === 'review') {
          savePhase.current = 'sealed'
          return saveStatus('sealed', -1, targets)
        }
        if (
          command.op === 'confirm_save' &&
          savePhase.current === 'sealed' &&
          command.ownerProof === ownerProof
        ) {
          savePhase.current = 'saved'
          return saveStatus('saved', -1, targets)
        }
        if (command.op === 'cancel' && savePhase.current !== 'saved') {
          savePhase.current = 'canceled'
          return saveStatus('canceled', -1, targets)
        }
        throw new Error('请在 Demo Inspector 中选「重置应用状态」后重试。')
      }}
    />
  )
}
