import { createRoot } from 'react-dom/client'
import { FirmwareTuningWorkspace } from '../../src/features/control-plane-demo/components/firmware-tuning-workspace'
import type { ThermalTuningStatus } from '../../src/features/control-plane-demo/contracts'

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
const root = document.getElementById('root')
const reads = document.querySelector('[data-testid="read-count"]')
const writes = document.querySelector('[data-testid="write-count"]')
if (!root || !reads || !writes) throw new Error('Fixture elements missing')
createRoot(root).render(
  <FirmwareTuningWorkspace
    supported
    deviceId={`polling-fixture-${crypto.randomUUID()}`}
    transport="mock"
    readStatus={async () => {
      readCount += 1
      reads.textContent = String(readCount)
      if (readCount === 1) return status
      throw new Error('Fixture tuning read unavailable')
    }}
    sendCommand={async (command) => {
      writeCount += 1
      writes.textContent = String(writeCount)
      writes.setAttribute('data-command', JSON.stringify(command))
      throw new Error('Fixture stop unavailable')
    }}
  />
)
