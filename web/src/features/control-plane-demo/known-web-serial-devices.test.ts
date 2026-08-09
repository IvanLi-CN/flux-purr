import { describe, expect, it } from 'vitest'
import {
  knownWebSerialDeviceToTarget,
  listKnownWebSerialDevices,
  rememberKnownWebSerialDevice,
} from './known-web-serial-devices'

class MemoryStorage {
  private readonly values = new Map<string, string>()

  getItem(key: string) {
    return this.values.get(key) ?? null
  }

  setItem(key: string, value: string) {
    this.values.set(key, value)
  }
}

describe('known browser Web Serial devices', () => {
  it('remembers a confirmed firmware identity without persisting runtime state', () => {
    const storage = new MemoryStorage()

    rememberKnownWebSerialDevice(
      {
        deviceId: 'a0f262f20d6c',
        hostname: 'flux-purr-a0f262f20d6c',
        firmwareVersion: '0.1.0',
        buildId: 'build-1',
      },
      storage
    )

    expect(listKnownWebSerialDevices(storage)).toEqual([
      {
        deviceId: 'a0f262f20d6c',
        hostname: 'flux-purr-a0f262f20d6c',
        firmwareVersion: '0.1.0',
        buildId: 'build-1',
      },
    ])
  })

  it('projects remembered identity as an offline Web Serial channel hint', () => {
    expect(
      knownWebSerialDeviceToTarget({
        deviceId: 'a0f262f20d6c',
        hostname: 'flux-purr-a0f262f20d6c',
        firmwareVersion: '0.1.0',
        buildId: 'build-1',
      })
    ).toMatchObject({
      id: 'web-serial-a0f262f20d6c',
      identityId: 'a0f262f20d6c',
      alias: 'flux-purr-a0f262f20d6c',
      transport: 'serial',
      severity: 'offline',
      leaseState: 'none',
      transportIssue: '选择此通道后，浏览器将验证已授权串口的设备身份。',
    })
  })

  it('keeps a confirmed identity when optional firmware metadata is absent', () => {
    const storage = new MemoryStorage()

    rememberKnownWebSerialDevice(
      {
        deviceId: 'a0f262f20d6c',
        hostname: 'flux-purr-a0f262f20d6c',
      } as unknown as Parameters<typeof rememberKnownWebSerialDevice>[0],
      storage
    )

    expect(listKnownWebSerialDevices(storage)).toEqual([
      {
        deviceId: 'a0f262f20d6c',
        hostname: 'flux-purr-a0f262f20d6c',
        firmwareVersion: 'unknown',
        buildId: 'unknown',
      },
    ])
  })

  it('rejects malformed browser memory instead of inventing a device', () => {
    const storage = new MemoryStorage()
    storage.setItem('flux-purr:known-web-serial-devices:v1', '{"deviceId":true}')

    expect(listKnownWebSerialDevices(storage)).toEqual([])
  })
})
