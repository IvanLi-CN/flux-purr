import { createRoot } from 'react-dom/client'
import '../../src/index.css'
import { ScrollArea } from '../../src/components/ui/scroll-area'
import { FirmwareTuningWorkspace } from '../../src/features/control-plane-demo/components/firmware-tuning-workspace'
import type { ThermalTuningStatus } from '../../src/features/control-plane-demo/contracts'
import { tuningOwnerSession } from '../../src/features/control-plane-demo/tuning-owner-session'

const trace = {
  firstSequence: 0,
  lastSequence: 0,
  complete: false,
  digest: 'sha256:fixture',
  gap: false,
}
const status: ThermalTuningStatus = {
  phase: 'warmup',
  trialPurpose: 'scout',
  terminalDisposition: 'running',
  reviewDisposition: 'pending',
  powerClass: 'pps3a',
  runId: 'polling-fixture-run',
  candidateId: 'polling-fixture-candidate',
  candidateHash: 'sha256:fixture-candidate',
  targetsC: [60],
  targetIndex: 0,
  armed: true,
  sealed: false,
  saveConfirmed: false,
  candidateProfile: null,
  previewReceipt: null,
  trace,
  events: {
    events: [
      {
        sequence: 0,
        elapsedMs: 0,
        kind: 'phase_transition',
        phase: 'warmup',
        targetC: 60,
        trialIndex: null,
        sample: null,
        reason: null,
        trial: null,
        decision: null,
      },
    ],
    nextSequence: 0,
    trace,
  },
  saveReceipt: null,
}
let readCount = 0
let writeCount = 0
const review = new URLSearchParams(location.search).get('review')
const chart = new URLSearchParams(location.search).has('chart')
const deviceId = `polling-fixture-${crypto.randomUUID()}`
const ownerProof = 'a'.repeat(64)
let phase: 'review' | 'sealed' | 'saved' | 'discarded' = 'review'
let sealAcknowledged = false
const identity = {
  runId: 'polling-fixture-run',
  candidateId: 'polling-fixture-candidate',
  candidateHash: 'sha256:fixture-candidate',
  powerClass: 'pps3a',
} as const
function chartStatus(afterSequence = -1): ThermalTuningStatus {
  const samples = [5800, 6220, 5900, 6000, 9900, 7800, 7990, 8050, 8010]
  const chartTrace = { ...trace, lastSequence: samples.length }
  const events: ThermalTuningStatus['events']['events'] = samples.map(
    (temperatureCentiC, sequence) => {
      const targetC = sequence < 4 ? 60 : 80
      const trialIndex = Math.min(2, Math.floor(sequence / 2))
      const phase = [
        'warmup',
        'hold',
        'warmup',
        'hold',
        'cooldown',
        'warmup',
        'approach',
        'hold',
        'hold',
      ][sequence] as ThermalTuningStatus['phase']
      const elapsedMs = sequence * 1000
      return {
        ...status.events.events[0],
        sequence,
        elapsedMs,
        targetC,
        trialIndex,
        kind: 'sample' as const,
        sample: {
          ...identity,
          sequence,
          elapsedMs,
          targetC,
          trialIndex,
          temperatureCentiC,
          vinMv: 15000,
          ppsContract: {
            voltageMv: 15000,
            currentMa: 3000,
            kind: 'pps3a' as const,
            confirmed: true,
          },
          requestedMv: 15000,
          ppsPowerCeilingMv: 21000,
          heaterRequestedOutputPermille: 250,
          heaterOutputPermille: 250,
          measurementValid: true,
          phase,
        },
      }
    }
  )
  events.push({
    ...status.events.events[0],
    sequence: samples.length,
    elapsedMs: samples.length * 1000,
    kind: 'decision',
    targetC: 60,
    trialIndex: 1,
    decision: { disposition: 'adopted', purpose: 'hold_confirm', holdPeakToPeakCentiC: 34 },
  })
  return {
    ...status,
    targetsC: [60, 80],
    targetIndex: 1,
    trace: chartTrace,
    events: {
      events: events.filter((event) => event.sequence > afterSequence),
      nextSequence: chartTrace.lastSequence,
      trace: chartTrace,
    },
  }
}
const reviewStatus = (afterSequence?: number): ThermalTuningStatus => {
  const sealed = phase === 'sealed'
  const saved = phase === 'saved'
  const discarded = phase === 'discarded'
  const lastSequence = sealed || saved ? 1 : 0
  const reviewTrace = { ...trace, complete: true, lastSequence }
  const first = status.events.events[0]
  return {
    ...status,
    phase: discarded ? 'canceled' : sealed ? 'sealed' : 'completed',
    terminalDisposition: discarded ? 'canceled' : saved ? 'passed' : 'candidate_ready',
    armed: false,
    sealed,
    saveConfirmed: saved,
    previewReceipt: discarded ? null : {},
    trace: reviewTrace,
    events: {
      events: [first, ...(lastSequence === 1 ? [{ ...first, sequence: 1 }] : [])].filter(
        (event) => event.sequence > (afterSequence ?? -1)
      ),
      nextSequence: lastSequence,
      trace: reviewTrace,
    },
    saveReceipt: saved
      ? {
          ...identity,
          bank: 'pps3a',
          bankRevision: 1,
          bankHash: 'sha256:saved',
          atomic: true,
          readbackVerified: true,
        }
      : null,
  }
}
if (review === 'owner')
  tuningOwnerSession.acceptResponse(deviceId, 'start', { ...status, ownerProof })
const root = document.getElementById('root')
const reads = document.querySelector('[data-testid="read-count"]')
const writes = document.querySelector('[data-testid="write-count"]')
if (!root || !reads || !writes) throw new Error('Fixture elements missing')
const workspace = (
  <FirmwareTuningWorkspace
    supported
    deviceId={deviceId}
    transport="mock"
    readStatus={async (cursor) => {
      readCount += 1
      reads.textContent = String(readCount)
      if (chart) return chartStatus(cursor?.afterSequence)
      if (review) {
        sealAcknowledged ||= phase === 'sealed' && cursor?.afterSequence === 1
        return reviewStatus(cursor?.afterSequence)
      }
      if (readCount === 1) return status
      throw new Error('Fixture tuning read unavailable')
    }}
    sendCommand={async (command) => {
      writeCount += 1
      writes.textContent = String(writeCount)
      if (review) {
        if (command.op === 'seal') phase = 'sealed'
        else if (command.op === 'cancel') phase = 'discarded'
        else if (
          command.op === 'confirm_save' &&
          sealAcknowledged &&
          command.ownerProof === ownerProof
        )
          phase = 'saved'
        else throw new Error('Save requires the acknowledged seal event and owner proof')
        return reviewStatus()
      }
      writes.setAttribute('data-command', JSON.stringify(command))
      throw new Error('Fixture stop unavailable')
    }}
  />
)
createRoot(root).render(
  chart ? (
    <div
      className="industrial-calibration-tabs__content"
      style={{ width: 'min(748px, 100%)', height: 600 }}
    >
      <ScrollArea className="industrial-calibration-tabs__firmware-scroll" autoHide>
        {workspace}
      </ScrollArea>
    </div>
  ) : (
    workspace
  )
)
