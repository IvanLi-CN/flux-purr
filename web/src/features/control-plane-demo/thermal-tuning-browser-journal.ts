import type { ThermalTuningEvent } from './contracts'
import type { ThermalTuningRecording } from './thermal-tuning-recorder'

const DATABASE = 'flux-purr-thermal-tuning-v2'
const VERSION = 2
const HISTORY_LIMIT = 10

interface StoredRun {
  deviceId: string
  runId: string
  recording: Omit<ThermalTuningRecording, 'events'>
  persistedThrough: number
}

interface StoredEvent {
  key: [string, string, number]
  event: ThermalTuningEvent
}

type ArchivedRun = StoredRun & { order: number }

let databasePromise: Promise<IDBDatabase> | null = null

function database(): Promise<IDBDatabase> {
  if (databasePromise) return databasePromise
  const opening = new Promise<IDBDatabase>((resolve, reject) => {
    if (typeof indexedDB === 'undefined') {
      reject(new Error('浏览器不支持调优事件持久化。'))
      return
    }
    const request = indexedDB.open(DATABASE, VERSION)
    request.onupgradeneeded = (event) => {
      const db = request.result
      if (!db.objectStoreNames.contains('events'))
        db.createObjectStore('events', { keyPath: 'key' })
      const history = db.createObjectStore('history', { keyPath: ['deviceId', 'runId'] })
      history.createIndex('order', 'order')
      history.createIndex('device', ['deviceId', 'order'])
      const upgrade = request.transaction
      if (event.oldVersion === 1 && upgrade) {
        const legacy = upgrade.objectStore('runs').openCursor()
        let order = 0
        legacy.onsuccess = () => {
          const cursor = legacy.result
          if (!cursor) {
            const keys = history.index('order').getAllKeys()
            keys.onsuccess = () => pruneHistory(history, upgrade.objectStore('events'), keys.result)
            return
          }
          const saved = cursor.value as StoredRun
          history.put({ ...saved, order: ++order } satisfies ArchivedRun)
          cursor.delete()
          cursor.continue()
        }
      }
    }
    request.onsuccess = () => {
      const db = request.result
      db.onversionchange = () => {
        db.close()
        databasePromise = null
      }
      resolve(db)
    }
    request.onerror = () => reject(request.error ?? new Error('无法打开调优事件日志。'))
    request.onblocked = () => reject(new Error('调优事件日志被其他页面占用。'))
  })
  const promise = opening.catch((error: unknown) => {
    databasePromise = null
    throw error
  })
  databasePromise = promise
  return promise
}

function result<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('调优事件日志读取失败。'))
  })
}

function committed(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve()
    transaction.onabort = () => reject(transaction.error ?? new Error('调优事件日志写入失败。'))
    transaction.onerror = () => reject(transaction.error ?? new Error('调优事件日志写入失败。'))
  })
}

export async function loadThermalTuningRecording(
  deviceId: string,
  runId?: string
): Promise<ThermalTuningRecording | null> {
  const db = await database()
  const transaction = db.transaction(['history', 'events'], 'readonly')
  const history = transaction.objectStore('history')
  const saved: ArchivedRun | undefined = runId
    ? await result(history.get([deviceId, runId]))
    : (
        await result(
          history
            .index('device')
            .openCursor(
              IDBKeyRange.bound([deviceId, 0], [deviceId, Number.MAX_SAFE_INTEGER]),
              'prev'
            )
        )
      )?.value
  if (!saved) return null
  const entries = (await result(
    transaction.objectStore('events').getAll(eventRange(deviceId, saved.runId))
  )) as StoredEvent[]
  return { ...saved.recording, events: entries.map((entry) => entry.event) }
}

function eventRange(deviceId: string, runId: string) {
  return IDBKeyRange.bound([deviceId, runId, 0], [deviceId, runId, Number.MAX_SAFE_INTEGER])
}

function pruneHistory(history: IDBObjectStore, events: IDBObjectStore, keys: IDBValidKey[]) {
  for (const key of keys.slice(0, Math.max(0, keys.length - HISTORY_LIMIT))) {
    const [deviceId, runId] = key as [string, string]
    history.delete(key)
    events.delete(eventRange(deviceId, runId))
  }
}

export async function persistThermalTuningRecording(
  deviceId: string,
  recording: ThermalTuningRecording
): Promise<void> {
  if (!recording.runId) throw new Error('调优事件日志缺少 runId。')
  const db = await database()
  const transaction = db.transaction(['history', 'events'], 'readwrite')
  const completion = committed(transaction)
  void completion.catch(() => {})
  try {
    const history = transaction.objectStore('history')
    const saved = (await result(history.get([deviceId, recording.runId]))) as
      | ArchivedRun
      | undefined
    const priorSequence = saved?.persistedThrough ?? -1
    const persistedThrough = recording.events.at(-1)?.sequence ?? -1
    if (persistedThrough < priorSequence) {
      await completion
      return
    }
    const events = transaction.objectStore('events')
    for (const event of recording.events) {
      if (event.sequence > priorSequence) {
        events.put({
          key: [deviceId, recording.runId, event.sequence],
          event,
        } satisfies StoredEvent)
      }
    }
    const { events: _events, status, ...withoutEvents } = recording
    const { ownerProof: _proof, ...publicStatus } = status as typeof status & {
      ownerProof?: unknown
    }
    // A browser tab may read an older run again, so only first observation sets its order.
    const orderIndex = history.index('order')
    const priorRun = saved ? null : await result(orderIndex.openKeyCursor(null, 'prev'))
    const order = saved?.order ?? Number(priorRun?.key ?? 0) + 1
    history.put({
      deviceId,
      runId: recording.runId,
      recording: {
        ...withoutEvents,
        status: {
          ...publicStatus,
          candidateProfile:
            publicStatus.candidateProfile ?? saved?.recording.status.candidateProfile ?? null,
        },
      },
      persistedThrough,
      order,
    } satisfies ArchivedRun)
    if (!saved) {
      pruneHistory(history, events, await result(orderIndex.getAllKeys()))
    }
    await completion
  } catch (error) {
    try {
      transaction.abort()
    } catch {
      // The transaction may already have aborted.
    }
    throw error
  }
}
