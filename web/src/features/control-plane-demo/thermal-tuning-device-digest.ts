import type { ThermalTuningEvent } from './contracts'

const textEncoder = new TextEncoder()
const DEVICE_EVENT_FIELDS: Record<string, string[]> = {
  event: [
    'sequence',
    'elapsedMs',
    'kind',
    'phase',
    'targetC',
    'trialIndex',
    'sample',
    'reason',
    'trial',
    'decision',
  ],
  'event.sample': [
    'sequence',
    'elapsedMs',
    'targetC',
    'trialIndex',
    'candidateId',
    'candidateHash',
    'temperatureCentiC',
    'vinMv',
    'ppsContract',
    'requestedMv',
    'ppsPowerCeilingMv',
    'heaterRequestedOutputPermille',
    'heaterOutputPermille',
    'measurementValid',
    'phase',
  ],
  'event.sample.ppsContract': ['voltageMv', 'currentMa', 'kind', 'confirmed'],
  'event.trial': [
    'candidate',
    'trialIndex',
    'purpose',
    'startSequence',
    'startElapsedMs',
    'endSequence',
    'endElapsedMs',
    'sampleStartSequence',
    'sampleEndSequence',
  ],
  'event.trial.candidate': [
    'candidateId',
    'candidateHash',
    'powerClass',
    'point',
    'seedSource',
    'lowerTargetC',
    'upperTargetC',
  ],
  'event.trial.candidate.point': [
    'targetC',
    'warmupPowerPermille',
    'warmupReenterCentiC',
    'brakeDistanceCentiC',
    'approachPowerPermille',
    'approachFloorPowerPermille',
    'approachDampingExponentPermille',
    'approachTailWindowCentiC',
    'holdPowerPermille',
    'holdReheatPowerPermille',
    'holdEntryCentiC',
    'holdExitCentiC',
    'holdOnCentiC',
    'holdOffCentiC',
    'overshootCutoffCentiC',
    'holdKpPermillePerC',
    'holdKiPermillePerCTick',
    'holdBlendTicks',
    'approachLeadTicks',
    'holdLeadTicks',
  ],
  'event.decision': [
    'disposition',
    'purpose',
    'maxOvershootCentiC',
    'holdPeakToPeakCentiC',
    'settleTimeMs',
    'score',
    'warmupFull',
    'approachToHold',
    'holdConfirm',
    'hardGateOvershootCentiC',
    'hardGateHoldPeakToPeakCentiC',
    'hardGateFullSpeedToStableMs',
    'freeze',
    'prune',
    'stopReason',
  ],
}

export async function digestEventChain(events: ThermalTuningEvent[]) {
  let previous = ''
  for (const event of events) {
    const encoded = textEncoder.encode(serializeDeviceValue(event, 'event'))
    const bytes = encoded.length <= 4096 ? encoded : fallbackEventBytes(event)
    const prefix = textEncoder.encode(previous)
    const combined = new Uint8Array(prefix.length + bytes.length)
    combined.set(prefix)
    combined.set(bytes, prefix.length)
    previous = (await sha256Bytes(combined)).slice(7)
  }
  return previous
}

function serializeDeviceValue(value: unknown, path: string): string {
  if (value === null || value === undefined) return 'null'
  if (Array.isArray(value)) {
    return `[${value.map((item) => serializeDeviceValue(item, `${path}[]`)).join(',')}]`
  }
  if (typeof value === 'number' && path === 'event.decision.score[]' && Number.isInteger(value)) {
    return `${value}.0`
  }
  if (typeof value !== 'object') return JSON.stringify(value)
  const object = value as Record<string, unknown>
  const keys = DEVICE_EVENT_FIELDS[path] ?? Object.keys(object)
  return `{${keys.map((key) => `${JSON.stringify(key)}:${serializeDeviceValue(object[key], `${path}.${key}`)}`).join(',')}}`
}

function fallbackEventBytes(event: ThermalTuningEvent) {
  const reason = textEncoder.encode(event.reason ?? '')
  const bytes = new Uint8Array(17 + reason.length)
  const view = new DataView(bytes.buffer)
  view.setUint32(0, event.sequence, true)
  view.setBigUint64(4, BigInt(event.elapsedMs), true)
  view.setUint8(
    12,
    ['sample', 'phase_transition', 'candidate_trial', 'decision', 'safety'].indexOf(event.kind)
  )
  view.setUint8(
    13,
    [
      'idle',
      'warmup',
      'approach',
      'hold',
      'cooldown',
      'sealed',
      'completed',
      'failed',
      'canceled',
    ].indexOf(event.phase)
  )
  view.setInt16(14, event.targetC ?? 0, true)
  view.setUint8(16, event.trialIndex ?? 0)
  bytes.set(reason, 17)
  return bytes
}

async function sha256Bytes(bytes: Uint8Array) {
  const digest = await crypto.subtle.digest('SHA-256', Uint8Array.from(bytes))
  return `sha256:${[...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, '0')).join('')}`
}
