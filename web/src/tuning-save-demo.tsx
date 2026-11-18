import { useRef, useState } from 'react'
import { FirmwareTuningWorkspace } from '@/features/control-plane-demo/components/firmware-tuning-workspace'
import type { ThermalTuningStatus } from '@/features/control-plane-demo/contracts'
import { ControlPlaneClientError } from '@/features/control-plane-demo/transport-client'
import { tuningOwnerSession } from '@/features/control-plane-demo/tuning-owner-session'

const ownerProof = 'a'.repeat(64)

export function TuningSaveScene() {
  const [deviceId] = useState(() => `demo-tuning-${crypto.randomUUID()}`)
  const phase = useRef<'review' | 'sealed' | 'saved' | 'canceled'>('review')
  const powerClass = 'pps3a' as const
  const identity = {
    runId: deviceId,
    candidateId: deviceId,
    candidateHash: `sha256:${'b'.repeat(64)}`,
    powerClass,
  }

  function snapshot(current: typeof phase.current, after = -1): ThermalTuningStatus {
    const phases = ['warmup', 'approach', 'hold', 'hold', 'hold'] as const
    const temperatures = [12800, 16950, 18060, 17975, 18010]
    const samples = temperatures.map((temperatureCentiC, sequence) => ({
      sequence,
      elapsedMs: sequence * 25_000,
      kind: 'sample' as const,
      targetC: 180,
      trialIndex: 0,
      phase: phases[sequence],
      reason: null,
      trial: null,
      decision: null,
      sample: {
        ...identity,
        sequence,
        elapsedMs: sequence * 25_000,
        targetC: 180,
        trialIndex: 0,
        phase: phases[sequence],
        temperatureCentiC,
        measurementValid: true,
        vinMv: 18000,
        requestedMv: 18000,
        ppsPowerCeilingMv: 21000,
        ppsContract: { voltageMv: 18000, currentMa: 3000, kind: powerClass, confirmed: true },
        heaterRequestedOutputPermille: 250,
        heaterOutputPermille: 250,
      },
    }))
    const events: ThermalTuningStatus['events']['events'] = [
      ...samples,
      {
        sequence: 5,
        elapsedMs: 100_000,
        kind: 'decision',
        phase: 'hold',
        targetC: 180,
        trialIndex: 0,
        sample: null,
        reason: null,
        trial: null,
        decision: { disposition: 'adopted', purpose: 'hold_confirm', holdPeakToPeakCentiC: 85 },
      },
    ]
    const saved = current === 'saved'
    const sealed = current === 'sealed'
    const canceled = current === 'canceled'
    if (sealed || saved)
      events.push({
        ...events[5],
        sequence: 6,
        kind: 'phase_transition',
        phase: 'sealed',
        decision: null,
      })
    if (saved)
      events.push({
        ...events[5],
        sequence: 7,
        kind: 'phase_transition',
        phase: 'completed',
        decision: null,
      })
    const trace = {
      firstSequence: 0,
      lastSequence: events.length - 1,
      complete: true,
      gap: false,
      digest: identity.candidateHash,
    }
    return {
      ...identity,
      phase: canceled ? 'canceled' : sealed ? 'sealed' : 'completed',
      terminalDisposition: saved ? 'passed' : canceled ? 'canceled' : 'candidate_ready',
      trialPurpose: 'hold_confirm',
      reviewDisposition: 'complete',
      targetsC: [180],
      targetIndex: 0,
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

  return (
    <FirmwareTuningWorkspace
      supported
      deviceId={deviceId}
      transport="mock"
      readStatus={async (cursor) => {
        if (phase.current === 'saved')
          throw new ControlPlaneClientError(
            'Mock response exceeded the LAN envelope.',
            'response_too_large',
            false
          )
        const next = snapshot(phase.current, cursor?.afterSequence)
        if (phase.current === 'review' && !tuningOwnerSession.proofFor(deviceId, deviceId)) {
          tuningOwnerSession.acceptResponse(deviceId, 'start', { ...next, ownerProof })
        }
        return next
      }}
      sendCommand={async (command) => {
        if (command.op === 'seal' && phase.current === 'review') {
          phase.current = 'sealed'
          return snapshot('sealed')
        }
        if (
          command.op === 'confirm_save' &&
          phase.current === 'sealed' &&
          command.ownerProof === ownerProof
        ) {
          phase.current = 'saved'
          return snapshot('saved')
        }
        if (command.op === 'cancel' && phase.current !== 'saved') {
          phase.current = 'canceled'
          return snapshot('canceled')
        }
        throw new Error('请在 Demo Inspector 中选「重置应用状态」后重试。')
      }}
    />
  )
}
