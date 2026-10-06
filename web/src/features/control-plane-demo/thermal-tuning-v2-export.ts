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
  annotateExecutionStatus(
    reportRuns,
    samples,
    ledger,
    status.targetsC,
    status.terminalDisposition,
    status.trace.gap
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
    generatedAt: Date.now(),
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
  const adopted = trials.find(
    (trial) => trial.disposition === 'adopted' && trial.reviewOutcome === 'passed'
  )
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

function annotateExecutionStatus(
  reportRuns: Array<Record<string, unknown>>,
  samples: JsonObject[],
  ledger: ThermalTuningEvent[],
  selectedTargets: number[],
  terminalDisposition: ThermalTuningStatus['terminalDisposition'],
  traceGap: boolean
) {
  const executionOrder = selectedTargets
  const hasEvidence = (target: number) =>
    samples.some((sample) => Number(sample.targetC) === target) ||
    reportRuns.some(
      (run) => Number(run.targetC) === target && Array.isArray(run.trials) && run.trials.length > 0
    )
  const lastAttemptedTarget = [...executionOrder].reverse().find(hasEvidence) ?? null
  const safetyEvent = [...ledger]
    .filter((event) => event.kind === 'safety')
    .sort(
      (left, right) =>
        (left.sequence ?? Number.MAX_SAFE_INTEGER) - (right.sequence ?? Number.MAX_SAFE_INTEGER)
    )[0]
  const terminalReason =
    safetyEvent?.reason ??
    (terminalDisposition === 'failed' ? (traceGap ? 'trace_gap' : 'run_terminated') : null)
  for (const run of reportRuns) {
    const target = Number(run.targetC)
    const attempted = hasEvidence(target)
    run.executionStatus = attempted ? 'attempted' : 'not_attempted'
    if (attempted) {
      run.blockedBy = null
      continue
    }
    run.stopReason = 'run_terminated_before_target'
    run.blockedBy = {
      targetC: lastAttemptedTarget,
      reason: selectedTargets.includes(target)
        ? (terminalReason ?? 'run_terminated_before_target')
        : 'outside_selected_scope',
    }
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
  const trialTiming = asObject(trial.timing)
  const decisionTiming = asObject(result.timing)
  const timing =
    trialTiming || decisionTiming
      ? trialTiming != null &&
        decisionTiming != null &&
        Object.keys(trialTiming).length === Object.keys(decisionTiming).length &&
        Object.keys(trialTiming).every((key) => trialTiming[key] === decisionTiming[key])
        ? decisionTiming
        : { availability: 'mismatched_device_timing_evidence' }
      : null
  const metrics = {
    warmupRequestedOutputPermille: warmupRequest,
    phasePath,
    maxOvershootC,
    holdPeakToPeakC,
    holdConfirmSeconds:
      trial.purpose === 'hold_confirm' &&
      result.purpose === 'hold_confirm' &&
      result.holdConfirm === true
        ? 60
        : null,
    fullSpeedToStable,
  }
  const report = {
    targetC: event.targetC,
    trialIndex: event.trialIndex,
    trialPurpose: trial.purpose ?? null,
    candidateId: candidate.candidateId ?? fallbackId,
    candidateHash: candidate.candidateHash ?? fallbackHash,
    disposition,
    reviewOutcome: 'failed',
    metrics,
    warmupRequestedOutputPermille: warmupRequest,
    maxOvershootC,
    holdPeakToPeakC,
    fullSpeedToStable,
    timing,
    timingEvidenceVerification: verifyTimingSamples(timing, event.targetC, trialSamples),
    cooldownGate: {
      startTemperatureCentiC: trial.startTemperatureCentiC ?? null,
      thresholdCentiC: trial.cooldownThresholdCentiC ?? null,
    },
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
  if (disposition === 'adopted' && trialPassesGates(report)) report.reviewOutcome = 'passed'
  return report
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
  const trial = run.trials.find(
    (item) => item.disposition === 'adopted' && item.reviewOutcome === 'passed'
  )
  return trial != null && trialPassesGates(trial)
}

function trialPassesGates(trial: ReturnType<typeof buildTrial>): boolean {
  const gate = trial.fullSpeedToStable
  return (
    trial.trialPurpose === 'hold_confirm' &&
    timingGateValid(trial) &&
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

function timingPolicyFloor(timing: JsonObject) {
  if (timing.policyVersion === 4) return 50
  if (timing.policyVersion === 2 || timing.policyVersion === 3) return 100
  return null
}

function supportedTimingPolicy(timing: JsonObject) {
  const floor = timingPolicyFloor(timing)
  return (
    floor != null &&
    timing.allowedApproachFloorPermille === floor &&
    ((timing.policyVersion === 2 && timing.modelApproachFloorPermille == null) ||
      ((timing.policyVersion === 3 || timing.policyVersion === 4) &&
        timing.modelApproachFloorPermille === floor &&
        timing.effectiveApproachFloorPermille === floor))
  )
}

function timingFloorEligible(timing: JsonObject) {
  return (
    supportedTimingPolicy(timing) &&
    timing.effectiveApproachFloorPermille === timingPolicyFloor(timing) &&
    (timing.modelApproachFloorPermille != null ||
      timing.configuredApproachFloorPermille === timingPolicyFloor(timing))
  )
}

export function timingGateValid(trial: JsonObject) {
  const timing = asObject(trial.timing)
  if (trial.timing == null) return false
  if (!timing) return false
  const point = asObject(asObject(trial.canonicalCandidate)?.point)
  const floor = numeric(point?.approachFloorPowerPermille)
  const hold = numeric(point?.holdPowerPermille)
  const target = numeric(trial.targetC)
  const original = target != null && target <= 150 ? 10_000 : 5_000
  const timestamp = (value: unknown) =>
    typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : null
  const timestampsValid = [
    'warmupExitedAtMs',
    'stableWindowStartedAtMs',
    'stableWindowVerifiedAtMs',
    'floorObservedAtMs',
    'extendedAtMs',
    'originalDeadlineObservedAtMs',
  ].every((key) => timing[key] == null || timestamp(timing[key]) != null)
  const exit = timestamp(timing.warmupExitedAtMs)
  const start = timestamp(timing.stableWindowStartedAtMs)
  const verified = timestamp(timing.stableWindowVerifiedAtMs)
  const observed = timestamp(timing.floorObservedAtMs)
  const extended = timestamp(timing.extendedAtMs)
  const gate = asObject(trial.fullSpeedToStable)
  const extensionValid =
    timing.extendedAtMs == null ||
    (extended != null &&
      observed != null &&
      exit != null &&
      observed >= exit &&
      observed < exit + original &&
      extended - exit >= original &&
      timing.originalDeadlineObservedAtMs === extended &&
      timing.originalDeadlinePhase === 'approach' &&
      timingFloorEligible(timing) &&
      timing.allowedApproachFloorPermille === timingPolicyFloor(timing))
  return (
    timestampsValid &&
    target != null &&
    target >= -32_768 &&
    target <= 32_767 &&
    Number.isInteger(target) &&
    supportedTimingPolicy(timing) &&
    timing.originalLimitMs === original &&
    timing.extensionLimitMs === original * 3 &&
    floor != null &&
    hold != null &&
    Number.isInteger(floor) &&
    Number.isInteger(hold) &&
    floor >= 0 &&
    floor <= 1_000 &&
    hold >= 0 &&
    hold <= 1_000 &&
    timing.allowedApproachFloorPermille === timingPolicyFloor(timing) &&
    timing.configuredApproachFloorPermille === floor &&
    timing.effectiveApproachFloorPermille ===
      (timing.modelApproachFloorPermille != null
        ? timingPolicyFloor(timing)
        : Math.max(floor, hold)) &&
    extensionValid &&
    asObject(trial.timingEvidenceVerification)?.verified === true &&
    gate?.limitMs === (extended == null ? original : original * 3) &&
    exit != null &&
    start != null &&
    verified != null &&
    start >= exit &&
    gate?.settleTimeMs === start - exit &&
    verified - start >= 10_000
  )
}

export function verifyTimingSamples(
  timing: JsonObject | null,
  target: unknown,
  samples: JsonObject[]
) {
  const reasons: string[] = []
  if (!timing || !supportedTimingPolicy(timing))
    return { verified: false, source: 'trial_samples', reasons: ['unsupported_timing_policy'] }
  const exit = numeric(timing.warmupExitedAtMs)
  const deadline = exit == null ? null : exit + Number(timing.originalLimitMs)
  const firstCheck =
    deadline == null ? undefined : samples.find((sample) => Number(sample.elapsedMs) >= deadline)
  if (
    (firstCheck?.elapsedMs ?? null) !== (timing.originalDeadlineObservedAtMs ?? null) ||
    (firstCheck?.phase ?? null) !== (timing.originalDeadlinePhase ?? null)
  )
    reasons.push('first_original_deadline_check_mismatch')
  const extended = numeric(timing.extendedAtMs)
  if (extended != null) {
    const witness = samples.find((sample) => sample.elapsedMs === timing.floorObservedAtMs)
    if (
      witness?.phase !== 'approach' ||
      witness.heaterRequestedOutputPermille !== timingPolicyFloor(timing) ||
      witness.measurementValid !== true
    )
      reasons.push(`missing_real_${timingPolicyFloor(timing)}_permille_floor_sample`)
    let stableStart: number | null = null
    let lastTime: number | null = null
    for (const sample of samples) {
      const time = numeric(sample.elapsedMs)
      if (time == null) continue
      if (time > extended) break
      if (
        sample.phase === 'approach' &&
        timing.floorObservedAtMs != null &&
        time >= Number(timing.floorObservedAtMs) &&
        sample.heaterRequestedOutputPermille !== 0 &&
        sample.heaterRequestedOutputPermille !== timingPolicyFloor(timing)
      )
        reasons.push('floor_request_raised_before_deadline')
      if (exit == null || time < exit) continue
      if (lastTime != null && time - lastTime > 1_000) stableStart = null
      lastTime = time
      if (
        sample.measurementValid === true &&
        typeof sample.temperatureCentiC === 'number' &&
        Math.abs(sample.temperatureCentiC - Number(target) * 100) <= 150
      )
        stableStart ??= time
      else stableStart = null
    }
    if (stableStart != null && deadline != null && stableStart <= deadline)
      reasons.push('timely_stable_window_at_original_deadline')
    if (
      !timingFloorEligible(timing) ||
      timing.allowedApproachFloorPermille !== timingPolicyFloor(timing) ||
      timing.originalDeadlinePhase !== 'approach' ||
      timing.originalDeadlineObservedAtMs !== extended ||
      exit == null ||
      deadline == null ||
      Number(timing.floorObservedAtMs) < exit ||
      timing.floorObservedAtMs == null ||
      Number(timing.floorObservedAtMs) >= deadline ||
      extended < deadline
    )
      reasons.push('invalid_fixed_floor_extension_identity')
  }
  const start = numeric(timing.stableWindowStartedAtMs)
  const verified = numeric(timing.stableWindowVerifiedAtMs)
  const window =
    start == null || verified == null
      ? []
      : samples.filter(
          (sample) => Number(sample.elapsedMs) >= start && Number(sample.elapsedMs) <= verified
        )
  const times = [...new Set(window.map((sample) => Number(sample.elapsedMs)))]
  if (
    start == null ||
    verified == null ||
    verified - start < 10_000 ||
    times[0] !== start ||
    times.at(-1) !== verified ||
    (times.length - 1) * 1_000 < (verified - start) * 3 ||
    times.some(
      (time, index) => index > 0 && (time - times[index - 1] > 1_000 || time <= times[index - 1])
    ) ||
    window.some(
      (sample) =>
        sample.measurementValid !== true ||
        typeof sample.temperatureCentiC !== 'number' ||
        Math.abs(sample.temperatureCentiC - Number(target) * 100) > 150
    )
  )
    reasons.push('stable_window_not_verified_from_samples')
  return { verified: reasons.length === 0, source: 'trial_samples', reasons }
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
