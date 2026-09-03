import { zipSync } from 'fflate'
import { thermalReportTemplate } from '@/generated/thermal-report/template'
import type { ThermalTuningRunSnapshot, ThermalTuningTraceEvent } from './contracts'

const DATABASE_NAME = 'flux-purr-thermal-tuning'
const STORE_NAME = 'runs'
const DATABASE_VERSION = 1
const memoryRuns = new Map<string, ThermalTuningRunSnapshot>()
const latestMemoryKeyByDevice = new Map<string, string>()

export interface ThermalTuningTraceHealth {
  expectedNextSequence: number
  observedThrough: number
  gap: boolean
  reviewIncomplete: boolean
}

export interface ThermalTuningBundleFiles {
  'index.html': string
  'run.bundle.json': string
  'samples.ndjson': string
  'thermal-profile.candidate.json': string
  'decision-ledger.ndjson': string
}

export function thermalTuningRunStorageKey(deviceId: string, runId: string) {
  return `${deviceId}:${runId}`
}

export function thermalTuningTraceHealth(
  snapshot: ThermalTuningRunSnapshot,
  persistedThrough?: number
): ThermalTuningTraceHealth {
  const events = [...snapshot.page.events].sort((left, right) => left.sequence - right.sequence)
  const initialExpected =
    persistedThrough === undefined && snapshot.run.state !== 'idle'
      ? 0
      : snapshot.page.earliestSequence
  let expected =
    persistedThrough === undefined
      ? initialExpected
      : Math.max(snapshot.page.earliestSequence, persistedThrough + 1)
  let gap = snapshot.page.earliestSequence > expected
  for (const event of events) {
    if (persistedThrough !== undefined && event.sequence <= persistedThrough) continue
    if (event.sequence !== expected) {
      gap = true
    }
    expected = Math.max(expected, event.sequence + 1)
  }
  const observedThrough = Math.max(
    persistedThrough ?? 0,
    snapshot.page.acknowledgedThrough ?? 0,
    ...events.map((event) => event.sequence)
  )
  const emittedThrough = snapshot.page.emittedThrough ?? 0
  if (emittedThrough > observedThrough) {
    gap = true
  }
  return {
    expectedNextSequence: expected,
    observedThrough,
    gap,
    reviewIncomplete: gap || snapshot.run.review.state === 'incomplete',
  }
}

function cloneSnapshot(snapshot: ThermalTuningRunSnapshot) {
  if (typeof structuredClone === 'function') return structuredClone(snapshot)
  return JSON.parse(JSON.stringify(snapshot)) as ThermalTuningRunSnapshot
}

function eventToJson(event: ThermalTuningTraceEvent) {
  return `${JSON.stringify(event)}\n`
}

function thermalTuningEvidenceComplete(events: ThermalTuningTraceEvent[]) {
  const targets = [60, 80, 100, 120, 140, 160, 180, 220, 240]
  const targetSet = (kind: ThermalTuningTraceEvent['kind']) =>
    new Set(events.filter((event) => event.kind === kind).map((event) => event.targetC))
  const samples = events.filter((event) => event.kind === 'sample')
  const trialKey = (event: ThermalTuningTraceEvent) =>
    `${event.targetC}:${event.trialIndex}:${event.candidateHash}:${event.canonicalCandidatePointHex}`
  const startedTrials = new Set(
    events
      .filter((event) => event.kind === 'candidate_trial' && event.eventReason === 'started')
      .map(trialKey)
  )
  const completedTrials = events.filter(
    (event) =>
      event.kind === 'candidate_trial' &&
      event.eventReason === 'completed' &&
      event.trialStartSequence != null &&
      event.trialEndSequence != null &&
      event.trialStartElapsedMs != null &&
      event.trialEndElapsedMs != null &&
      event.gates != null
  )
  const completedTrialKeys = new Set(completedTrials.map(trialKey))
  const allTargetsPresent = (observed: Set<number | null | undefined>) =>
    targets.every((target) => observed.has(target))
  const contiguous = events.every((event, expected) => event.sequence === expected)

  return (
    samples.length > 0 &&
    samples.every(
      (event) => event.targetC != null && event.trialIndex != null && event.candidateHash != null
    ) &&
    allTargetsPresent(new Set(samples.map((event) => event.targetC))) &&
    allTargetsPresent(targetSet('phase_transition')) &&
    allTargetsPresent(targetSet('decision')) &&
    completedTrials.length > 0 &&
    startedTrials.size === completedTrialKeys.size &&
    [...startedTrials].every((key) => completedTrialKeys.has(key)) &&
    contiguous
  )
}

function reportPhase(phase?: string | null) {
  if (phase === 'scout') return 'warmup'
  if (phase === 'retune') return 'approach'
  if (phase === 'hold_confirm') return 'hold'
  return 'cooldown_wait'
}

function candidatePoint(hex?: string | null) {
  if (!hex || hex.length !== 80) return null
  const bytes = Uint8Array.from(hex.match(/.{2}/g) ?? [], (pair) => Number.parseInt(pair, 16))
  if (bytes.length !== 40 || bytes.some((value) => !Number.isFinite(value))) return null
  const view = new DataView(bytes.buffer)
  const values = Array.from({ length: 19 }, (_, index) => view.getUint16(2 + index * 2, true))
  const keys = [
    'brakeDistanceCentiC',
    'warmupPowerPermille',
    'warmupReenterCentiC',
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
  ]
  return Object.fromEntries([
    ['targetTempC', view.getInt16(0, true)],
    ...keys.map((key, index) => [key, values[index]]),
  ])
}

function eventResult(event: ThermalTuningTraceEvent) {
  return {
    stopReason: event.disposition ?? event.eventReason ?? null,
    maxOvershootC: event.scoreOvershoot == null ? null : event.scoreOvershoot / 100,
    holdPeakToPeakC: event.scoreStability == null ? null : event.scoreStability / 100,
    scoreSettleMs: event.scoreSettleMs ?? null,
  }
}

function decisionFacts(event: ThermalTuningTraceEvent) {
  return {
    gates: event.gates ?? null,
    candidateFrozen: event.candidateFrozen ?? null,
    intervalLowerBoundaryC: event.intervalLowerBoundaryC ?? null,
    intervalUpperBoundaryC: event.intervalUpperBoundaryC ?? null,
    intervalPruned: event.intervalPruned ?? null,
    scoreTracking: event.scoreTracking ?? null,
    scoreEnergy: event.scoreEnergy ?? null,
    scoreHoldMeanAbsoluteErrorCenti: event.scoreHoldMeanAbsoluteErrorCenti ?? null,
    scoreOutputSwitches: event.scoreOutputSwitches ?? null,
  }
}

function firmwareReportData(
  deviceId: string,
  snapshot: ThermalTuningRunSnapshot,
  complete: boolean
) {
  const events = [...snapshot.page.events].sort((left, right) => left.sequence - right.sequence)
  const samples = events.filter((event) => event.kind === 'sample')
  const decisions = new Map(
    events
      .filter((event) => event.kind === 'decision' && event.targetC != null)
      .map((event) => [event.targetC, event])
  )
  const rawRuns = [60, 80, 100, 120, 140, 160, 180, 220, 240].flatMap((target) => {
    const decision = decisions.get(target)
    if (!decision) return []
    const trials = events.filter(
      (event) =>
        event.kind === 'candidate_trial' &&
        event.eventReason === 'completed' &&
        event.targetC === target
    )
    const rounds = trials.map((trial) => {
      const trialSamples = samples.filter(
        (sample) => sample.targetC === target && sample.trialIndex === trial.trialIndex
      )
      const started = trialSamples[0]?.elapsedMs ?? trial.trialStartElapsedMs ?? 0
      return {
        round: (trial.trialIndex ?? 0) + 1,
        attemptType: 'firmware',
        candidateName: trial.candidateId ?? null,
        candidateHash: trial.candidateHash ?? null,
        selected: trial.candidateHash === decision.candidateHash,
        evidenceValid: trial.gates != null && (trial.gates & 0x0f) === 0x0f,
        point: candidatePoint(trial.canonicalCandidatePointHex),
        pointSource: 'firmware_candidate_trial',
        samples: trialSamples.map((sample) => ({
          t: (sample.elapsedMs - started) / 1_000,
          temp: sample.temperatureCentiC == null ? null : sample.temperatureCentiC / 100,
          output: sample.heaterOutputPermille == null ? null : sample.heaterOutputPermille / 10,
          requestV: sample.ppsContractMv == null ? null : sample.ppsContractMv / 1_000,
          vinV: sample.vinMv == null ? null : sample.vinMv / 1_000,
          ppsContractCurrentA: sample.ppsContractMa == null ? null : sample.ppsContractMa / 1_000,
          phase: reportPhase(sample.phase),
          firmwarePhase: sample.phase,
          heaterPhase: sample.heaterPhase,
          measurementValid: sample.measurementValid,
          sequence: sample.sequence,
        })),
        result: eventResult(trial),
        firmwareDecision: decisionFacts(trial),
      }
    })
    const allSamples = rounds.flatMap((round) => round.samples)
    const selected = rounds.find((round) => round.selected)
    return [
      {
        target,
        targetRole: 'tuning',
        attemptType: 'firmware',
        reviewPassed: decision.disposition === 'accepted',
        reviewOutcome: decision.disposition === 'accepted' ? 'passed' : decision.disposition,
        candidateDisposition: decision.disposition,
        candidateReady: decision.disposition === 'accepted',
        timeSpentSeconds:
          (Math.max(...trials.map((trial) => trial.trialEndElapsedMs ?? 0), 0) -
            Math.min(...trials.map((trial) => trial.trialStartElapsedMs ?? 0), 0)) /
          1_000,
        validTestCount: rounds.filter((round) => round.evidenceValid).length,
        invalidTestCount: rounds.filter((round) => !round.evidenceValid).length,
        roundCount: rounds.length,
        samples: allSamples,
        rounds,
        result: eventResult(decision),
        firmwareDecision: decisionFacts(decision),
        point: selected?.point ?? null,
        pointSource: 'firmware_candidate_trial',
        failures: [],
      },
    ]
  })
  return {
    reportKind: 'firmware_tuning_v2',
    omitUnavailableFields: true,
    reportCapabilities: {
      sourceTelemetry: false,
      commandTelemetry: false,
      filteredTemperature: false,
      controlTemperature: false,
    },
    eyebrow: 'Flux Purr / Firmware-owned PPS thermal tuning',
    title: `Flux Purr ${(snapshot.run.powerClass ?? 'PPS').toUpperCase()} 固件热控调优报告`,
    subtitle:
      '设备执行九点 PPS 调优。报告保留设备温度、加热输出、阶段、候选参数与决策账本；未采集的外部 Source 遥测不会显示。',
    generatedAt: Date.now(),
    selectedMode: 'firmware',
    resolvedBank: snapshot.run.powerClass,
    deviceId,
    runId: snapshot.run.runId,
    terminalDisposition: snapshot.run.terminalDisposition,
    reviewDisposition: complete ? 'complete' : 'incomplete',
    candidate: snapshot.run.candidate,
    trace: snapshot.page,
    tuningBudgetSeconds: 1_200,
    tuningTargetsC: [60, 80, 100, 120, 140, 160, 180, 220, 240],
    tuningExecutionOrderC: [60, 240, 140, 100, 80, 120, 180, 160, 220],
    metaItems: [
      ['运行模式', 'firmware'],
      ['PPS 等级', snapshot.run.powerClass],
      ['Run ID', snapshot.run.runId],
      ['终态', snapshot.run.terminalDisposition],
      ['审查', complete ? 'complete' : 'incomplete'],
      ['候选状态', complete ? snapshot.run.candidate.promotionState : 'unavailable'],
    ],
    stampItems: [
      ['DEVICE', deviceId],
      ['REPORT', Date.now()],
    ],
    bundleFiles: [
      'index.html',
      'run.bundle.json',
      'samples.ndjson',
      'thermal-profile.candidate.json',
      'decision-ledger.ndjson',
    ],
    runs: rawRuns,
    rawRuns,
    history: [],
    run: snapshot.run,
  }
}

function renderFirmwareReport(
  deviceId: string,
  snapshot: ThermalTuningRunSnapshot,
  complete: boolean
) {
  const data = JSON.stringify(firmwareReportData(deviceId, snapshot, complete))
    .replaceAll('&', '\\u0026')
    .replaceAll('<', '\\u003c')
    .replaceAll('>', '\\u003e')
    .replaceAll('\u2028', '\\u2028')
    .replaceAll('\u2029', '\\u2029')
  return thermalReportTemplate.replace('__THERMAL_REPORT_DATA__', data)
}

export function buildThermalTuningBundle(
  deviceId: string,
  snapshot: ThermalTuningRunSnapshot
): ThermalTuningBundleFiles {
  const events = [...snapshot.page.events].sort((left, right) => left.sequence - right.sequence)
  const samples = events
    .filter((event) => event.kind === 'sample')
    .map(eventToJson)
    .join('')
  const decisions = events
    .filter((event) => event.kind !== 'sample')
    .map(eventToJson)
    .join('')
  const firstSequence = events[0]?.sequence ?? 0
  const lastSequence = Math.max(events.at(-1)?.sequence ?? 0, snapshot.page.emittedThrough ?? 0)
  const evidenceComplete = thermalTuningEvidenceComplete(events)
  const complete = !thermalTuningTraceHealth(snapshot).reviewIncomplete && evidenceComplete
  const reviewDisposition =
    snapshot.run.review.state === 'complete' && complete ? 'complete' : 'incomplete'
  const candidate = {
    schema: 'thermal-tuning-v2',
    deviceId,
    runId: snapshot.run.runId,
    candidateId: snapshot.run.candidate.candidateId ?? null,
    candidateHash: snapshot.run.candidate.candidateHash ?? null,
    candidate: snapshot.run.candidate,
    powerClass: snapshot.run.powerClass ?? null,
    reviewDisposition,
    promotionState: complete ? snapshot.run.candidate.promotionState : 'unavailable',
    promotionReceipts: snapshot.hostPromotionReceipts ?? [],
  }
  return {
    'index.html': renderFirmwareReport(deviceId, snapshot, complete),
    'run.bundle.json': JSON.stringify(
      {
        schema: 'thermal-tuning-v2',
        runId: snapshot.run.runId,
        engine: 'firmware',
        powerClass: snapshot.run.powerClass ?? null,
        physicalTargetsC: [60, 80, 100, 120, 140, 160, 180, 220, 240],
        executionOrderC: [60, 240, 140, 100, 80, 120, 180, 160, 220],
        terminalDisposition: snapshot.run.terminalDisposition ?? null,
        reviewDisposition,
        trace: {
          firstSequence,
          lastSequence,
          complete,
          digest: snapshot.page.digestThroughPage ?? null,
          gap: complete ? null : evidenceComplete ? 'trace_gap' : 'evidence_incomplete',
        },
        candidate: snapshot.run.candidate,
        referenceComparison: 'not_run',
        deviceId,
        run: snapshot.run,
        tracePage: snapshot.page,
        files: {
          'index.html': null,
          'run.bundle.json': null,
          'samples.ndjson': null,
          'thermal-profile.candidate.json': null,
          'decision-ledger.ndjson': null,
        },
      },
      null,
      2
    ),
    'samples.ndjson': samples,
    'thermal-profile.candidate.json': JSON.stringify(candidate, null, 2),
    'decision-ledger.ndjson': decisions,
  }
}

function encodeFiles(files: ThermalTuningBundleFiles) {
  return zipSync(
    Object.fromEntries(
      Object.entries(files).map(([name, content]) => [name, new TextEncoder().encode(content)])
    )
  )
}

export function thermalTuningBundleBlob(deviceId: string, snapshot: ThermalTuningRunSnapshot) {
  return new Blob([encodeFiles(buildThermalTuningBundle(deviceId, snapshot))], {
    type: 'application/zip',
  })
}

export function downloadThermalTuningBundle(deviceId: string, snapshot: ThermalTuningRunSnapshot) {
  if (typeof document === 'undefined') return false
  const url = URL.createObjectURL(thermalTuningBundleBlob(deviceId, snapshot))
  const link = document.createElement('a')
  link.href = url
  link.download = `${deviceId}-${snapshot.run.runId}-thermal-tuning-v2.zip`
  link.click()
  window.setTimeout(() => URL.revokeObjectURL(url), 0)
  return true
}

function openDatabase() {
  if (typeof indexedDB === 'undefined') return null
  return new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DATABASE_NAME, DATABASE_VERSION)
    request.onupgradeneeded = () => {
      request.result.createObjectStore(STORE_NAME)
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('IndexedDB unavailable'))
  })
}

export async function persistThermalTuningSnapshot(
  deviceId: string,
  snapshot: ThermalTuningRunSnapshot
) {
  const key = thermalTuningRunStorageKey(deviceId, snapshot.run.runId)
  memoryRuns.set(key, cloneSnapshot(snapshot))
  latestMemoryKeyByDevice.set(deviceId, key)
  const database = await openDatabase()
  if (!database) return
  await new Promise<void>((resolve, reject) => {
    const transaction = database.transaction(STORE_NAME, 'readwrite')
    transaction.objectStore(STORE_NAME).put(snapshot, key)
    transaction.oncomplete = () => resolve()
    transaction.onerror = () => reject(transaction.error ?? new Error('IndexedDB write failed'))
    transaction.onabort = () => reject(transaction.error ?? new Error('IndexedDB write aborted'))
  })
  database.close()
}

function latestSnapshot(values: ThermalTuningRunSnapshot[]) {
  return values
    .filter((snapshot) => snapshot.run.runId !== 'idle')
    .sort((left, right) => {
      const leftId = Number(left.run.runId)
      const rightId = Number(right.run.runId)
      if (Number.isFinite(leftId) && Number.isFinite(rightId)) return rightId - leftId
      return right.run.runId.localeCompare(left.run.runId)
    })[0]
}

export async function loadLatestThermalTuningSnapshot(deviceId: string) {
  const memoryKey = latestMemoryKeyByDevice.get(deviceId)
  const memorySnapshot = memoryKey ? memoryRuns.get(memoryKey) : undefined
  const database = await openDatabase()
  if (!database) return memorySnapshot ? cloneSnapshot(memorySnapshot) : null

  try {
    const entries = await new Promise<Array<{ key: string; snapshot: ThermalTuningRunSnapshot }>>(
      (resolve, reject) => {
        const values: Array<{ key: string; snapshot: ThermalTuningRunSnapshot }> = []
        const request = database
          .transaction(STORE_NAME, 'readonly')
          .objectStore(STORE_NAME)
          .openCursor()
        request.onsuccess = () => {
          const cursor = request.result
          if (!cursor) {
            resolve(values)
            return
          }
          if (typeof cursor.key === 'string' && cursor.key.startsWith(`${deviceId}:`)) {
            values.push({
              key: cursor.key,
              snapshot: cursor.value as ThermalTuningRunSnapshot,
            })
          }
          cursor.continue()
        }
        request.onerror = () => reject(request.error ?? new Error('IndexedDB read failed'))
      }
    )
    const stored = entries
      .filter(({ key }) => key.startsWith(`${deviceId}:`))
      .map(({ snapshot }) => snapshot)
    const selected = latestSnapshot(stored) ?? memorySnapshot
    return selected ? cloneSnapshot(selected) : null
  } catch {
    return memorySnapshot ? cloneSnapshot(memorySnapshot) : null
  } finally {
    database.close()
  }
}

export async function loadThermalTuningSnapshot(deviceId: string, runId: string) {
  const key = thermalTuningRunStorageKey(deviceId, runId)
  const database = await openDatabase()
  if (!database) return memoryRuns.get(key) ?? null
  const value = await new Promise<ThermalTuningRunSnapshot | null>((resolve, reject) => {
    const transaction = database.transaction(STORE_NAME, 'readonly')
    const request = transaction.objectStore(STORE_NAME).get(key)
    request.onsuccess = () =>
      resolve((request.result as ThermalTuningRunSnapshot | undefined) ?? null)
    request.onerror = () => reject(request.error ?? new Error('IndexedDB read failed'))
  })
  database.close()
  return value ?? memoryRuns.get(key) ?? null
}
