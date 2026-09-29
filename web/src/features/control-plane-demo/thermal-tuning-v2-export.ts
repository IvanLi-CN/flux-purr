import { zipSync } from 'fflate'
import reportTemplate from '../../../../tools/flux-purr-devd/src/bin/flux_purr/thermal_tuning_v2_report_template.html?raw'
import type { ThermalTuningEvent, ThermalTuningStatus } from './contracts'
import { digestEventChain } from './thermal-tuning-device-digest'
import type { ThermalTuningRecording } from './thermal-tuning-recorder'

const PHYSICAL_TARGETS = [60, 80, 100, 120, 140, 160, 180, 220, 240]
const EXECUTION_ORDER = [60, 240, 140, 100, 80, 120, 180, 160, 220]
const textEncoder = new TextEncoder()
const requiredSampleFields = [
  'sequence',
  'elapsedMs',
  'kind',
  'targetC',
  'trialIndex',
  'candidateId',
  'candidateHash',
  'temperatureCentiC',
  'vinMv',
  'ppsContractMv',
  'ppsContractMa',
  'ppsContractKind',
  'ppsContractConfirmed',
  'ppsRequestedMv',
  'ppsPowerCeilingMv',
  'heaterRequestedOutputPermille',
  'heaterOutputPermille',
  'measurementValid',
  'phase',
]

type JsonObject = Record<string, unknown>

export interface ThermalTuningV2Export {
  files: Record<string, string>
  archive: Uint8Array
  ownerOutcome: 'passed' | 'failed'
}

export async function exportThermalTuningV2(
  recording: ThermalTuningRecording,
  deviceId: string,
  transport: string
): Promise<ThermalTuningV2Export> {
  const status = recording.status
  if (!recording.runId || !status.runId || recording.runId !== status.runId) {
    throw new Error('没有可导出的设备运行。')
  }
  const events = [...recording.events].sort((left, right) => left.sequence - right.sequence)
  const samples = events.filter((event) => event.kind === 'sample').map(flattenSample)
  const ledger = events.filter((event) => event.kind !== 'sample')
  const verification = await verifyDeviceTrace(recording, events)
  const reportRuns = PHYSICAL_TARGETS.map((target) =>
    buildReportRun(target, samples, ledger, status.candidateId, status.candidateHash)
  )
  const fullScope =
    EXECUTION_ORDER.every((target, index) => status.targetsC[index] === target) &&
    status.targetsC.length === EXECUTION_ORDER.length
  const candidateState = receiptIdentityVerified(
    status.runId,
    status.candidateId,
    status.candidateHash,
    asObject(status.previewReceipt),
    asObject(status.saveReceipt)
  )
    ? 'saved'
    : status.terminalDisposition === 'candidate_ready'
      ? status.sealed
        ? 'sealed'
        : 'candidate_ready'
      : 'rejected'
  const candidate = {
    schema: 'thermal-tuning-v2',
    runId: status.runId,
    candidateId: status.candidateId,
    candidateHash: status.candidateHash,
    powerClass: status.powerClass,
    promotion: {
      state: candidateState,
      previewReceipt: status.previewReceipt,
      saveReceipt: status.saveReceipt,
    },
    profile: {
      bank: status.candidateProfile ? status.powerClass : null,
      source: status.candidateProfile ? 'device_reported' : 'unavailable',
      value: status.candidateProfile,
    },
  }
  const eventAudit = await auditEvents(samples, ledger, status.trace, verification.verified)
  const safetySummary = buildSafetySummary(ledger, status.terminalDisposition, status.trace.gap)
  const qualified =
    status.terminalDisposition === 'passed' &&
    status.powerClass === 'pps5a' &&
    fullScope &&
    recording.complete &&
    eventAudit.trace.complete &&
    verification.verified &&
    fullProfileAvailable(status.candidateProfile) &&
    reportRuns.every((run) => run.reviewPassed && adoptedTrialPassesGate(run)) &&
    promotionVerified(status.runId, status.candidateId, status.candidateHash, candidate)
  const ownerOutcome = qualified ? 'passed' : 'failed'
  const promotionInconsistent =
    (status.terminalDisposition === 'passed' || status.saveReceipt != null) &&
    !promotionVerified(status.runId, status.candidateId, status.candidateHash, candidate)
  const reviewIncomplete =
    !eventAudit.trace.complete || !verification.verified || promotionInconsistent
  const ppsBounds = status.ppsVoltageBounds
  const bundle: JsonObject = {
    schema: 'thermal-tuning-v2',
    runId: status.runId,
    engine: 'firmware',
    device: { id: deviceId, transport },
    firmware: { source: 'device_identity', identity: 'device_reported' },
    powerClass: status.powerClass,
    sourceClass: 'device-local',
    ppsControlPolicyVersion: status.ppsControlPolicyVersion ?? null,
    ppsVoltageBounds: {
      absoluteMinMv: ppsBounds?.absoluteMinMv ?? null,
      absoluteMaxMv: ppsBounds?.absoluteMaxMv ?? null,
      policyFloorMv: ppsBounds?.policyFloorMv ?? null,
      apdoMinMv: ppsBounds?.apdoMinMv ?? null,
      apdoMaxMv: ppsBounds?.apdoMaxMv ?? null,
      availability: !ppsBounds
        ? 'unavailable_device_status_field'
        : ppsBounds.apdoMinMv != null && ppsBounds.apdoMaxMv != null
          ? 'device_local'
          : 'unavailable_selected_apdo',
    },
    targetSet: {
      physicalC: PHYSICAL_TARGETS,
      selectedC: [...status.targetsC].sort((left, right) => left - right),
      executionOrderC: status.targetsC,
    },
    qualificationScope: {
      kind: fullScope ? 'default-pps5a-full-batch' : 'diagnostic-retest',
      targetCount: status.targetsC.length,
      diagnosticOnly: !fullScope,
    },
    terminalDisposition: status.terminalDisposition,
    reviewDisposition: reviewIncomplete ? 'review_incomplete' : status.reviewDisposition,
    deviceTrace: status.trace,
    deviceTraceVerification: verification,
    reportRuns,
    rawRuns: reportRuns,
    trace: eventAudit.trace,
    eventCounts: eventAudit.counts,
    safetySummary,
    candidate: {
      candidateId: status.candidateId,
      candidateHash: status.candidateHash,
      powerClass: status.powerClass,
      promotionState: candidateState,
    },
    referenceComparison: {
      fixture: 'external-5a-conformance-v1',
      status: 'pending_device_conformance_evidence',
      algorithmCopied: false,
    },
    capability: {
      localEvidence: {
        vin: samples.length ? 'available' : 'unavailable',
        ppsContract: samples.length ? 'available' : 'unavailable',
        externalVbus: 'unavailable',
        physicalPower: 'unavailable',
      },
    },
    generatedBy: 'flux-purr web thermal-tuning-v2',
    generatedAt: new Date().toISOString(),
    ownerOutcome,
    files: {
      'index.html': 'index.html',
      'run.bundle.json': 'run.bundle.json',
      'samples.ndjson': 'samples.ndjson',
      'thermal-profile.candidate.json': 'thermal-profile.candidate.json',
      'decision-ledger.ndjson': 'decision-ledger.ndjson',
    },
  }
  const files: Record<string, string> = {
    'samples.ndjson': ndjson(samples),
    'decision-ledger.ndjson': ndjson(ledger),
    'thermal-profile.candidate.json': prettyJson(candidate),
  }
  const htmlData = { bundle: { ...bundle, fileDigests: {} }, samples, ledger, candidate }
  files['index.html'] = reportTemplate.replace(
    '__THERMAL_TUNING_V2_DATA__',
    JSON.stringify(htmlData).replace(
      /[&<>\u2028\u2029]/g,
      (character) => `\\u${character.charCodeAt(0).toString(16).padStart(4, '0')}`
    )
  )
  const digests: Record<string, string | null> = {}
  for (const [name, content] of Object.entries(files)) digests[name] = await sha256(content)
  bundle.fileDigests = { ...digests, 'run.bundle.json': null }
  bundle.fileDigestBasis = 'run.bundle.json hashes itself with its own digest field omitted'
  digests['run.bundle.json'] = await sha256(prettyJson(bundle))
  bundle.fileDigests = { ...digests }
  files['run.bundle.json'] = prettyJson(bundle)
  const archive = zipSync(
    Object.fromEntries(
      Object.entries(files).map(([name, content]) => [name, textEncoder.encode(content)])
    )
  )
  return { files, archive, ownerOutcome }
}

function flattenSample(event: ThermalTuningEvent): JsonObject {
  const sample = event.sample
  if (!sample) return { sequence: event.sequence, kind: 'sample', availability: 'sample_missing' }
  const { ppsContract, requestedMv, ...rest } = sample
  return {
    ...rest,
    kind: 'sample',
    ppsContractMv: ppsContract?.voltageMv ?? null,
    ppsContractMa: ppsContract?.currentMa ?? null,
    ppsContractKind: ppsContract?.kind ?? null,
    ppsContractConfirmed: ppsContract?.confirmed ?? null,
    ppsRequestedMv: requestedMv ?? null,
    pidPhase: sample.phase,
  }
}

function buildReportRun(
  targetC: number,
  samples: JsonObject[],
  ledger: ThermalTuningEvent[],
  fallbackId: string | null,
  fallbackHash: string | null
) {
  const trialsByIndex = new Map<number, ThermalTuningEvent>()
  for (const event of ledger) {
    if (event.kind !== 'candidate_trial' || event.targetC !== targetC || event.trialIndex == null)
      continue
    const previous = trialsByIndex.get(event.trialIndex)
    if (event.trial?.endSequence != null || !previous) trialsByIndex.set(event.trialIndex, event)
  }
  const trials = [...trialsByIndex.values()]
    .sort((left, right) => (left.trialIndex ?? 0) - (right.trialIndex ?? 0))
    .map((event) => buildTrial(event, samples, ledger, fallbackId, fallbackHash))
  const adopted = trials.find((trial) => trial.disposition === 'adopted')
  const summary = adopted ?? trials.at(-1)
  return {
    targetC,
    reviewOutcome: adopted ? 'passed' : 'failed',
    reviewPassed: Boolean(adopted),
    adoptedTrialIndex: adopted?.trialIndex ?? null,
    trialCount: trials.length,
    metrics: summary?.metrics ?? {},
    trials,
  }
}

function buildSafetySummary(
  ledger: ThermalTuningEvent[],
  terminalDisposition: ThermalTuningStatus['terminalDisposition'],
  traceGap: boolean
): JsonObject | null {
  const event = [...ledger]
    .filter((item) => item.kind === 'safety')
    .sort(
      (left, right) =>
        (left.sequence ?? Number.MAX_SAFE_INTEGER) - (right.sequence ?? Number.MAX_SAFE_INTEGER)
    )[0]
  if (event) {
    return {
      kind: 'safety',
      source: 'device_event',
      reason: event.reason ?? 'device_safety_event',
      sequence: event.sequence,
      elapsedMs: event.elapsedMs,
      phase: event.phase,
      heaterState: null,
      origin: null,
    }
  }
  if (terminalDisposition === 'failed') {
    return {
      kind: 'safety',
      source: traceGap ? 'trace_audit' : 'device_terminal',
      reason: traceGap ? 'trace_gap' : 'device_terminal_failed_without_safety_event',
      sequence: null,
      elapsedMs: null,
      phase: 'failed',
      heaterState: 'unconfirmed',
      origin: 'report_audit',
    }
  }
  return null
}

function buildTrial(
  event: ThermalTuningEvent,
  samples: JsonObject[],
  ledger: ThermalTuningEvent[],
  fallbackId: string | null,
  fallbackHash: string | null
) {
  const trial = event.trial ?? {}
  const candidate = asObject(trial.candidate) ?? {
    candidateId: fallbackId,
    candidateHash: fallbackHash,
    powerClass: 'pps5a',
  }
  const decision = ledger.find(
    (item) =>
      item.kind === 'decision' &&
      item.targetC === event.targetC &&
      item.trialIndex === event.trialIndex &&
      item.decision != null
  )
  const result = decision?.decision ?? {}
  const disposition = typeof result.disposition === 'string' ? result.disposition : 'rejected'
  const trialSamples = samples.filter(
    (sample) =>
      sample.targetC === event.targetC &&
      sample.trialIndex === event.trialIndex &&
      sample.phase !== 'cooldown' &&
      (trial.sampleStartSequence == null ||
        Number(sample.sequence) >= Number(trial.sampleStartSequence)) &&
      (trial.sampleEndSequence == null ||
        Number(sample.sequence) <= Number(trial.sampleEndSequence))
  )
  const warmup = trialSamples
    .filter((sample) => sample.phase === 'warmup')
    .map((sample) => Number(sample.heaterRequestedOutputPermille))
    .filter(Number.isFinite)
  const phasePath = trialSamples
    .reduce<string[]>((phases, sample) => {
      const phase = String(sample.phase)
      if (phases.at(-1) !== phase) phases.push(phase)
      return phases
    }, [])
    .join(' -> ')
  const first = trialSamples.at(0)
  const last = trialSamples.at(-1)
  const warmupRequest = warmup.length ? Math.max(...warmup) : null
  const overshoot = numeric(result.maxOvershootCentiC)
  const holdPeakToPeak = numeric(result.holdPeakToPeakCentiC)
  const maxOvershootC = overshoot == null ? null : overshoot / 100
  const holdPeakToPeakC = holdPeakToPeak == null ? null : holdPeakToPeak / 100
  const fullSpeedToStable = {
    settleTimeMs: result.settleTimeMs ?? null,
    limitMs: result.hardGateFullSpeedToStableMs ?? null,
  }
  const metrics = {
    warmupRequestedOutputPermille: warmupRequest,
    phasePath,
    maxOvershootC,
    holdPeakToPeakC,
    holdConfirmSeconds: result.holdConfirm === true ? 60 : null,
    fullSpeedToStable,
  }
  return {
    trialIndex: event.trialIndex,
    candidateId: candidate.candidateId ?? fallbackId,
    candidateHash: candidate.candidateHash ?? fallbackHash,
    disposition,
    reviewOutcome: disposition === 'adopted' ? 'passed' : 'failed',
    metrics,
    warmupRequestedOutputPermille: warmupRequest,
    maxOvershootC,
    holdPeakToPeakC,
    fullSpeedToStable,
    firstSequence: first?.sequence ?? null,
    lastSequence: last?.sequence ?? null,
    sampleStartSequence: trial.sampleStartSequence ?? null,
    sampleEndSequence: trial.sampleEndSequence ?? null,
    firstElapsedMs: first?.elapsedMs ?? null,
    lastElapsedMs: last?.elapsedMs ?? null,
    trialStartSequence: trial.startSequence ?? null,
    trialEndSequence: trial.endSequence ?? null,
    bounds: {
      lowerTargetC: candidate.lowerTargetC ?? null,
      upperTargetC: candidate.upperTargetC ?? null,
    },
    candidateSeed: {
      source: candidate.seedSource ?? 'device_candidate_preview',
      lowerTargetC: candidate.lowerTargetC ?? null,
      upperTargetC: candidate.upperTargetC ?? null,
    },
    canonicalCandidate: candidate,
    stopReason: result.stopReason ?? event.reason ?? 'device_terminal_failure',
  }
}

async function verifyDeviceTrace(recording: ThermalTuningRecording, events: ThermalTuningEvent[]) {
  const trace = recording.status.trace
  const reasons: string[] = []
  if (!recording.complete) reasons.push('final event range not acknowledged')
  if (recording.gap || trace.gap || !trace.complete) reasons.push('device trace incomplete')
  if (trace.firstSequence !== 0 || trace.lastSequence !== events.at(-1)?.sequence) {
    reasons.push('device trace sequence range does not match collected events')
  }
  if (events.some((event, index) => event.sequence !== index))
    reasons.push('device trace has a missing sequence')
  if ((await digestEventChain(events)) !== trace.digest)
    reasons.push('device trace digest does not match collected events')
  return { verified: reasons.length === 0, reasons }
}

async function auditEvents(
  samples: JsonObject[],
  ledger: ThermalTuningEvent[],
  deviceTrace: ThermalTuningRecording['status']['trace'],
  deviceVerified: boolean
) {
  const events = [...samples, ...ledger].sort(
    (left, right) => Number(left.sequence) - Number(right.sequence)
  )
  const reasons: string[] = []
  if (!deviceVerified) reasons.push('firmware device trace was not verified')
  const counts: Record<string, number> = {}
  for (const [index, event] of events.entries()) {
    const kind = String(event.kind)
    counts[kind] = (counts[kind] ?? 0) + 1
    if (event.sequence !== index) reasons.push(`missing or duplicate sequence ${index}`)
    if (kind === 'sample') {
      const sample = event as JsonObject
      for (const field of requiredSampleFields) {
        if (!(field in sample)) reasons.push(`sequence ${event.sequence} sample missing ${field}`)
      }
      for (const field of ['heaterRequestedOutputPermille', 'heaterOutputPermille']) {
        const value = sample[field]
        if (typeof value !== 'number' || !Number.isInteger(value) || value < 0 || value > 1000)
          reasons.push(`sequence ${event.sequence} invalid ${field}`)
      }
      const ceiling = sample.ppsPowerCeilingMv
      const requested = sample.ppsRequestedMv
      if (
        typeof ceiling !== 'number' ||
        typeof requested !== 'number' ||
        !Number.isInteger(ceiling) ||
        !Number.isInteger(requested) ||
        ceiling < 5000 ||
        ceiling > 28000 ||
        requested > ceiling
      ) {
        reasons.push(`sequence ${event.sequence} invalid PPS request voltage ceiling (mV)`)
      }
    }
  }
  if (events.at(0)?.sequence !== 0 || events.at(-1)?.sequence !== deviceTrace.lastSequence) {
    reasons.push('report event range does not match verified device trace')
  }
  const digest = await sha256(ndjson(events))
  return {
    trace: {
      firstSequence: events.at(0)?.sequence ?? null,
      lastSequence: events.at(-1)?.sequence ?? null,
      complete: reasons.length === 0,
      digest,
      gap: reasons.length ? { reason: 'trace_gap', details: reasons } : null,
    },
    counts,
  }
}

function promotionVerified(
  runId: string,
  candidateId: string | null,
  candidateHash: string | null,
  candidate: JsonObject
) {
  const promotion = asObject(candidate.promotion)
  const preview = asObject(promotion?.previewReceipt)
  const save = asObject(promotion?.saveReceipt)
  return (
    promotion?.state === 'saved' &&
    receiptIdentityVerified(runId, candidateId, candidateHash, preview, save)
  )
}

function receiptIdentityVerified(
  runId: string,
  candidateId: string | null,
  candidateHash: string | null,
  preview: JsonObject | null,
  save: JsonObject | null
) {
  if (!preview || !save) return false
  return (
    [preview, save].every(
      (receipt) =>
        receipt.runId === runId &&
        receipt.candidateId === candidateId &&
        receipt.candidateHash === candidateHash &&
        receipt.powerClass === 'pps5a'
    ) &&
    preview.ramAppliedHash === candidateHash &&
    save.bank === 'pps5a' &&
    save.bankHash === candidateHash &&
    save.atomic === true &&
    save.readbackVerified === true &&
    Number.isInteger(save.bankRevision)
  )
}

function fullProfileAvailable(value: unknown) {
  const profile = asObject(value)
  const points = profile?.points
  return (
    asObject(profile?.settings) != null &&
    Array.isArray(points) &&
    PHYSICAL_TARGETS.every((target) =>
      points.some((point) => asObject(point)?.targetTempC === target)
    )
  )
}

function adoptedTrialPassesGate(run: ReturnType<typeof buildReportRun>) {
  const trial = run.trials.find((item) => item.disposition === 'adopted')
  if (!trial) return false
  const gate = trial.fullSpeedToStable
  return (
    trial.warmupRequestedOutputPermille === 1_000 &&
    trial.metrics.phasePath.includes('approach -> hold') &&
    trial.maxOvershootC != null &&
    trial.maxOvershootC <= 3 &&
    trial.holdPeakToPeakC != null &&
    trial.holdPeakToPeakC <= 3 &&
    trial.metrics.holdConfirmSeconds === 60 &&
    typeof gate.settleTimeMs === 'number' &&
    typeof gate.limitMs === 'number' &&
    gate.settleTimeMs <= gate.limitMs
  )
}

function asObject(value: unknown): JsonObject | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonObject) : null
}

function numeric(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function ndjson(values: unknown[]) {
  return values.map((value) => JSON.stringify(value)).join('\n') + (values.length ? '\n' : '')
}

function prettyJson(value: unknown) {
  return `${JSON.stringify(value, null, 2)}\n`
}

async function sha256(value: string) {
  return sha256Bytes(textEncoder.encode(value))
}

async function sha256Bytes(bytes: Uint8Array) {
  const digest = await crypto.subtle.digest('SHA-256', Uint8Array.from(bytes))
  return `sha256:${[...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, '0')).join('')}`
}
