import { afterEach, describe, expect, it, vi } from 'vitest'

import { startDevdFirmwareProgressMonitor } from './firmware-workbench-logic'

class FakeEventSource {
  static instances: FakeEventSource[] = []
  readonly listeners = new Map<string, Set<(event: MessageEvent<string>) => void>>()
  onopen: (() => void) | null = null
  onerror: (() => void) | null = null
  closed = false

  readonly url: string

  constructor(url: string) {
    this.url = url
    FakeEventSource.instances.push(this)
    queueMicrotask(() => this.onopen?.())
  }

  addEventListener(type: string, listener: (event: MessageEvent<string>) => void) {
    const listeners = this.listeners.get(type) ?? new Set()
    listeners.add(listener)
    this.listeners.set(type, listeners)
  }

  removeEventListener(type: string, listener: (event: MessageEvent<string>) => void) {
    this.listeners.get(type)?.delete(listener)
  }

  emit(type: string, payload: unknown) {
    const message = { data: JSON.stringify(payload) } as MessageEvent<string>
    for (const listener of this.listeners.get(type) ?? []) listener(message)
  }

  close() {
    this.closed = true
  }
}

describe('devd firmware progress monitor', () => {
  const originalEventSource = globalThis.EventSource

  afterEach(() => {
    vi.restoreAllMocks()
    FakeEventSource.instances = []
    Object.defineProperty(globalThis, 'EventSource', {
      configurable: true,
      value: originalEventSource,
    })
    Reflect.deleteProperty(globalThis, 'window')
  })

  it('filters backlog and de-duplicates SSE replay sequences', async () => {
    Object.defineProperty(globalThis, 'EventSource', {
      configurable: true,
      value: FakeEventSource,
    })
    Object.defineProperty(globalThis, 'window', {
      configurable: true,
      value: globalThis,
    })
    vi.spyOn(Date, 'now').mockReturnValue(2_000)
    const events: Array<{ sequence: number }> = []
    const monitor = startDevdFirmwareProgressMonitor({
      devdBaseUrl: 'http://127.0.0.1:30080',
      deviceId: 'device-1',
      phase: 'execution',
      operation: 'update',
      artifactId: 'sha256:bundle',
      onEvent: (event) => events.push(event),
    })
    await monitor.ready
    monitor.arm()
    const source = FakeEventSource.instances[0]
    const envelope = (timestamp: string, sequence: number, event: string) => ({
      timestamp,
      kind: 'firmware_operation',
      payload: {
        schemaVersion: 1,
        operationId: 'operation-1',
        phase: 'execution',
        operation: 'update',
        artifactId: 'sha256:bundle',
        sequence,
        event,
      },
    })

    source.emit('firmware_operation', envelope('1500', 1, 'operation_started'))
    source.emit('firmware_operation', envelope('2000', 1, 'operation_started'))
    source.emit('firmware_operation', envelope('2001', 2, 'stage_started'))
    source.emit('firmware_operation', envelope('2001', 2, 'stage_started'))

    expect(events.map((event) => event.sequence)).toEqual([1, 2])
    monitor.close()
    expect(source.closed).toBe(true)
  })
})
