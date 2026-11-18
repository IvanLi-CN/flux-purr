import { describe, expect, it, vi } from 'vitest'
import { TuningOwnerSession } from './tuning-owner-session'

function storage() {
  const values = new Map<string, string>()
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => void values.set(key, value),
    removeItem: (key: string) => void values.delete(key),
  }
}

describe('tuning initiator ownership', () => {
  const proof = 'ab'.repeat(32)

  it('keeps the proof out of public status and binds ownership to device and run', () => {
    const owner = new TuningOwnerSession()
    const status = owner.acceptResponse('device-a', 'start', {
      runId: 'run-a',
      phase: 'warmup',
      ownerProof: proof,
      candidateHash: 'public-hash',
    })
    expect(status).toEqual({ runId: 'run-a', phase: 'warmup', candidateHash: 'public-hash' })
    expect(JSON.stringify(status)).not.toContain(proof)
    expect(owner.proofFor('device-a', 'run-a')).toBe(proof)
    expect(owner.proofFor('device-b', 'run-a')).toBeUndefined()
    expect(owner.proofFor('device-a', 'run-b')).toBeUndefined()
  })

  it('restores this tab after reload while another client cannot claim a read response', () => {
    const tab = storage()
    const owner = new TuningOwnerSession(() => tab)
    owner.acceptResponse('device-a', 'start', { runId: 'run-a', ownerProof: proof })
    expect(new TuningOwnerSession(() => tab).proofFor('device-a', 'run-a')).toBe(proof)
    const observer = new TuningOwnerSession(() => storage())
    expect(
      observer.acceptResponse('device-a', 'read', { runId: 'run-a', ownerProof: proof })
    ).toEqual({ runId: 'run-a' })
    expect(observer.proofFor('device-a', 'run-a')).toBeUndefined()
  })

  it('forgets the old run after observing a replacement run, including after reload', () => {
    const tab = storage()
    new TuningOwnerSession(() => tab).acceptResponse('device-a', 'start', {
      runId: 'old-run',
      ownerProof: proof,
    })
    const reloaded = new TuningOwnerSession(() => tab)
    reloaded.acceptResponse('device-a', 'read', { runId: 'new-run' })
    expect(reloaded.proofFor('device-a', 'old-run')).toBeUndefined()
    expect(new TuningOwnerSession(() => tab).proofFor('device-a', 'old-run')).toBeUndefined()
    expect(reloaded.proofFor('device-a', 'new-run')).toBeUndefined()
  })

  it.each([
    { phase: 'failed' },
    { phase: 'canceled' },
    { saveConfirmed: true },
  ])('removes terminal ownership from memory and tab storage: %o', (terminal) => {
    const tab = storage()
    const owner = new TuningOwnerSession(() => tab)
    owner.acceptResponse('device-a', 'start', { runId: 'run-a', ownerProof: proof })
    owner.acceptResponse('device-a', 'read', { runId: 'run-a', ...terminal })
    expect(owner.proofFor('device-a', 'run-a')).toBeUndefined()
    expect(new TuningOwnerSession(() => tab).proofFor('device-a', 'run-a')).toBeUndefined()
  })

  it('fails closed for malformed proofs and keeps RAM ownership if storage is unavailable', () => {
    const owner = new TuningOwnerSession(() => {
      throw new Error('storage disabled')
    })
    owner.acceptResponse('device-a', 'start', { runId: 'bad', ownerProof: 'not-a-proof' })
    expect(owner.proofFor('device-a', 'bad')).toBeUndefined()
    owner.acceptResponse('device-a', 'start', { runId: 'run-a', ownerProof: proof })
    expect(owner.proofFor('device-a', 'run-a')).toBe(proof)
    owner.acceptResponse('device-a', 'cancel', { runId: 'run-a', phase: 'canceled' })
    expect(owner.proofFor('device-a', 'run-a')).toBeUndefined()
  })

  it('forwards the private proof only on save and strips it before recording a start response', async () => {
    const owner = new TuningOwnerSession()
    const start = vi.fn(async () => ({ runId: 'run-a', phase: 'warmup', ownerProof: proof }))
    const status = await owner.command('device-a', { op: 'start', ownerProof: 'untrusted' }, start)
    expect(start).toHaveBeenCalledWith({ op: 'start' })
    expect(status).toEqual({ runId: 'run-a', phase: 'warmup' })
    const save = vi.fn(async () => ({ runId: 'run-a', phase: 'passed', saveConfirmed: true }))
    await owner.command('device-a', { op: 'confirm_save', runId: 'run-a' }, save)
    expect(save).toHaveBeenCalledWith({ op: 'confirm_save', runId: 'run-a', ownerProof: proof })
    expect(owner.proofFor('device-a', 'run-a')).toBeUndefined()
  })

  it('prevents a second client from issuing a save even if it supplies a copied public identity', async () => {
    const observer = new TuningOwnerSession()
    const send = vi.fn(async () => ({ runId: 'run-a' }))
    await expect(
      observer.command(
        'device-a',
        {
          op: 'confirm_save',
          runId: 'run-a',
          ownerProof: proof,
        },
        send
      )
    ).rejects.toThrow('只有发起本轮调优的页面可以确认保存')
    expect(send).not.toHaveBeenCalled()
    await observer.command('device-a', { op: 'cancel', runId: 'run-a' }, send)
    expect(send).toHaveBeenCalledWith({ op: 'cancel', runId: 'run-a' })
  })
})
