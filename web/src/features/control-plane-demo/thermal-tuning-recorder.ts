import { zipSync } from 'fflate'
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
    .filter((event) => event.kind === 'decision')
    .map(eventToJson)
    .join('')
  const candidate = {
    schema: 'thermal-tuning-v2',
    deviceId,
    runId: snapshot.run.runId,
    candidateId: snapshot.run.candidate.candidateId ?? null,
    candidateHash: snapshot.run.candidate.candidateHash ?? null,
    candidate: snapshot.run.candidate,
    powerClass: snapshot.run.powerClass ?? null,
    reviewDisposition: snapshot.run.review.state === 'complete' ? 'complete' : 'incomplete',
    promotionState: snapshot.run.candidate.promotionState,
  }
  const firstSequence = events[0]?.sequence ?? 0
  const lastSequence = Math.max(events.at(-1)?.sequence ?? 0, snapshot.page.emittedThrough ?? 0)
  const complete = !thermalTuningTraceHealth(snapshot).reviewIncomplete
  return {
    'index.html': `<!doctype html><meta charset="utf-8"><title>Flux Purr thermal tuning</title><main><h1>Flux Purr 热控调优</h1><p>Run ${snapshot.run.runId} · ${snapshot.run.powerClass ?? 'unknown'}</p><p>Review: ${snapshot.run.review.state} · Trace: ${complete ? 'complete' : 'incomplete'}</p></main>`,
    'run.bundle.json': JSON.stringify(
      {
        schema: 'thermal-tuning-v2',
        runId: snapshot.run.runId,
        engine: 'firmware',
        powerClass: snapshot.run.powerClass ?? null,
        physicalTargetsC: [60, 80, 100, 120, 140, 160, 180, 220, 240],
        executionOrderC: [60, 240, 140, 100, 80, 120, 180, 160, 220],
        terminalDisposition: snapshot.run.terminalDisposition ?? null,
        reviewDisposition: snapshot.run.review.state === 'complete' ? 'complete' : 'incomplete',
        trace: {
          firstSequence,
          lastSequence,
          complete,
          digest: snapshot.page.digestThroughPage ?? null,
          gap: complete ? null : 'trace_gap',
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
