import type { ThermalTuningEvent } from './contracts'
import type { ThermalTuningRecording } from './thermal-tuning-recorder'

const DATABASE = 'flux-purr-thermal-tuning-v2'
const VERSION = 1

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

let databasePromise: Promise<IDBDatabase> | null = null

function database(): Promise<IDBDatabase> {
  if (databasePromise) return databasePromise
  const opening = new Promise<IDBDatabase>((resolve, reject) => {
    if (typeof indexedDB === 'undefined') {
      reject(new Error('浏览器不支持调优事件持久化。'))
      return
    }
    const request = indexedDB.open(DATABASE, VERSION)
    request.onupgradeneeded = () => {
      const db = request.result
      if (!db.objectStoreNames.contains('runs'))
        db.createObjectStore('runs', { keyPath: 'deviceId' })
      if (!db.objectStoreNames.contains('events'))
        db.createObjectStore('events', { keyPath: 'key' })
    }
    request.onsuccess = () => resolve(request.result)
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
  deviceId: string
): Promise<ThermalTuningRecording | null> {
  const db = await database()
  const transaction = db.transaction(['runs', 'events'], 'readonly')
  const saved = (await result(transaction.objectStore('runs').get(deviceId))) as
    | StoredRun
    | undefined
  if (!saved) return null
  const range = IDBKeyRange.bound(
    [deviceId, saved.runId, 0],
    [deviceId, saved.runId, Number.MAX_SAFE_INTEGER]
  )
  const entries = (await result(transaction.objectStore('events').getAll(range))) as StoredEvent[]
  return { ...saved.recording, events: entries.map((entry) => entry.event) }
}

export async function persistThermalTuningRecording(
  deviceId: string,
  recording: ThermalTuningRecording
): Promise<void> {
  if (!recording.runId) throw new Error('调优事件日志缺少 runId。')
  const db = await database()
  const transaction = db.transaction(['runs', 'events'], 'readwrite')
  const completion = committed(transaction)
  try {
    const runs = transaction.objectStore('runs')
    const saved = (await result(runs.get(deviceId))) as StoredRun | undefined
    const priorSequence = saved?.runId === recording.runId ? saved.persistedThrough : -1
    const events = transaction.objectStore('events')
    for (const event of recording.events) {
      if (event.sequence > priorSequence) {
        events.put({
          key: [deviceId, recording.runId, event.sequence],
          event,
        } satisfies StoredEvent)
      }
    }
    const { events: _events, ...withoutEvents } = recording
    runs.put({
      deviceId,
      runId: recording.runId,
      recording: withoutEvents,
      persistedThrough: recording.events.at(-1)?.sequence ?? -1,
    } satisfies StoredRun)
    await completion
  } catch (error) {
    transaction.abort()
    throw error
  }
}
