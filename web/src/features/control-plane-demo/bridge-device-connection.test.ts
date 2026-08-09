import { describe, expect, it } from 'vitest'
import {
  bridgeCandidatesForTransport,
  validateBridgeDeviceIdentity,
} from './bridge-device-connection'
import { devdLanSummaryToBridgeTarget } from './components/control-plane-demo'
import type { Identity } from './contracts'
import type { DeviceTarget } from './types'

const identity: Identity = {
  deviceId: 'a0f262f20d6c',
  hostname: 'flux-purr-a0f262f20d6c',
  firmwareVersion: '0.1.0',
  buildId: 'local-build',
  gitSha: '0123456789abcdef',
  board: 'esp32s3_frontpanel',
  apiVersion: '2026-05-29',
  protocolVersion: 'flux-purr.usb.v1',
  capabilities: ['identity', 'network', 'status'],
}

describe('bridge device identity validation', () => {
  it('accepts only a complete Flux Purr firmware identity', () => {
    expect(validateBridgeDeviceIdentity(identity)).toEqual({ ok: true })
  })

  it.each([
    ['missing device id', { deviceId: '' }],
    ['wrong API', { apiVersion: 'v0' }],
    ['wrong protocol', { protocolVersion: 'other.usb.v1' }],
    ['missing control capability', { capabilities: ['identity', 'status'] }],
  ])('rejects %s as an unknown device', (_label, override) => {
    expect(validateBridgeDeviceIdentity({ ...identity, ...override })).toEqual({
      ok: false,
      reason: 'unknown_device',
    })
  })

  it('keeps an established DEVD LAN target out of the WiFi connection candidates', () => {
    const registeredLanCandidate = {
      id: 'lan-a0f262f20d6c',
      transport: 'devd',
      bridgeTransport: 'wifi',
      connectionAvailable: false,
      connectionCandidate: true,
    } as DeviceTarget
    const establishedBridgeTarget = {
      ...registeredLanCandidate,
      id: 'devd-lan-a0f262f20d6c',
      connectionAvailable: true,
      connectionCandidate: false,
    } as DeviceTarget
    const usbCandidate = {
      ...registeredLanCandidate,
      id: 'serial-303a-1001-A0:F2:62:F2:0D:6C',
      bridgeTransport: 'usb',
    } as DeviceTarget

    expect(
      bridgeCandidatesForTransport({
        transport: 'wifi',
        devices: [establishedBridgeTarget, usbCandidate],
        lanDevices: [registeredLanCandidate],
      }).map((device) => device.id)
    ).toEqual(['lan-a0f262f20d6c'])
  })

  it('marks a registered DEVD LAN summary as a connectable candidate', () => {
    expect(
      devdLanSummaryToBridgeTarget({
        id: 'lan-a0f262f20d6c',
        baseUrl: 'http://192.168.31.189',
        hostname: 'flux-purr-a0f262f20d6c',
        lastIpv4: '192.168.31.189',
        paired: true,
      }).connectionCandidate
    ).toBe(true)
  })
})
