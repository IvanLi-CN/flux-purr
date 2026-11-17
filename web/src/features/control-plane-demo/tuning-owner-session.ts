type OwnerStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>
type OwnerStatus = { runId?: string | null; phase?: string; saveConfirmed?: boolean }

const prefix = 'flux-purr.tuning-owner.'
const validProof = (value: unknown): value is string =>
  typeof value === 'string' && /^[a-f0-9]{64}$/.test(value)

/** Separate from the trace journal: only this tab's command path can claim a run. */
export class TuningOwnerSession {
  private readonly memory = new Map<string, string>()
  private readonly activeRuns = new Map<string, string>()
  private readonly storage: () => OwnerStorage | undefined

  constructor(storage: () => OwnerStorage | undefined = () => undefined) {
    this.storage = storage
  }

  private key(deviceId: string, runId: string) {
    return `${prefix}${encodeURIComponent(deviceId)}.${encodeURIComponent(runId)}`
  }

  proofFor(deviceId: string, runId: string | null | undefined): string | undefined {
    if (!runId) return undefined
    const key = this.key(deviceId, runId)
    const remembered = this.memory.get(key)
    if (remembered) return remembered
    try {
      const saved = this.storage()?.getItem(key)
      if (validProof(saved)) {
        this.memory.set(key, saved)
        return saved
      }
    } catch {
      // Storage restrictions do not discard this tab's in-memory ownership.
    }
    return undefined
  }

  forget(deviceId: string, runId: string) {
    const key = this.key(deviceId, runId)
    this.memory.delete(key)
    try {
      this.storage()?.removeItem(key)
    } catch {
      // The device still invalidates the proof on terminal transitions.
    }
  }

  async command<
    C extends { op: string; runId?: string; ownerProof?: string },
    S extends OwnerStatus,
  >(
    deviceId: string,
    command: C,
    send: (command: C) => Promise<S>
  ): Promise<Omit<S, 'ownerProof'>> {
    // Ignore caller-supplied ownership. Only a successful start response can
    // grant this tab the right to save; a public run identity cannot do so.
    const { ownerProof: _untrusted, ...publicCommand } = command
    const proof = command.op === 'confirm_save' ? this.proofFor(deviceId, command.runId) : undefined
    if (command.op === 'confirm_save' && !proof) {
      throw new Error('只有发起本轮调优的页面可以确认保存，请回到发起页面。')
    }
    const response = await send({ ...publicCommand, ...(proof ? { ownerProof: proof } : {}) } as C)
    return this.acceptResponse(deviceId, command.op, response)
  }

  /** Strip the secret before a response reaches React state, journals or reports. */
  acceptResponse<T extends OwnerStatus>(
    deviceId: string,
    op: string,
    response: T & { ownerProof?: unknown }
  ): Omit<T, 'ownerProof'> {
    const { ownerProof, ...publicStatus } = response
    const runId = response.runId
    if (!runId) return publicStatus
    const activeKey = `${prefix}${encodeURIComponent(deviceId)}.active-run`
    let previousRun = this.activeRuns.get(deviceId)
    try {
      previousRun ??= this.storage()?.getItem(activeKey) ?? undefined
      this.storage()?.setItem(activeKey, runId)
    } catch {
      // In-memory isolation also works without tab storage.
    }
    if (previousRun && previousRun !== runId) this.forget(deviceId, previousRun)
    this.activeRuns.set(deviceId, runId)
    if (op === 'start' && validProof(ownerProof)) {
      const key = this.key(deviceId, runId)
      this.memory.set(key, ownerProof)
      try {
        this.storage()?.setItem(key, ownerProof)
      } catch {
        // Keep ownership in RAM when tab storage is unavailable.
      }
    }
    if (response.saveConfirmed || response.phase === 'failed' || response.phase === 'canceled') {
      this.forget(deviceId, runId)
    }
    return publicStatus
  }
}

export const tuningOwnerSession = new TuningOwnerSession(() => window.sessionStorage)
