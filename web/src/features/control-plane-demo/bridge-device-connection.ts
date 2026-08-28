import {
  CONTROL_PLANE_API_VERSION,
  type ControlPlaneStatus,
  type Identity,
  type NetworkSummary,
  USB_PROTOCOL_VERSION,
} from './contracts'
import type { DeviceTarget } from './types'

const REQUIRED_BRIDGE_CAPABILITIES = ['identity', 'network', 'status'] as const

export type BridgeTransportChoice = 'usb' | 'wifi'

/**
 * A `devd-lan-*` record is already an established control target.  It is not
 * a saved LAN endpoint and must never be sent back to DEVD's LAN connect API.
 */
export function bridgeCandidatesForTransport({
  transport,
  devices,
  lanDevices,
}: {
  transport: BridgeTransportChoice
  devices: readonly DeviceTarget[]
  lanDevices: readonly DeviceTarget[]
}): DeviceTarget[] {
  const source = transport === 'wifi' ? lanDevices : devices
  const candidates = new Map<string, DeviceTarget>()

  for (const device of source) {
    if (
      !(device.connectionAvailable || device.connectionCandidate) ||
      device.transport !== 'devd' ||
      device.bridgeTransport !== transport
    ) {
      continue
    }

    const existing = candidates.get(device.id)
    if (!existing || bridgeCandidatePriority(device) < bridgeCandidatePriority(existing)) {
      candidates.set(device.id, device)
    }
  }

  return [...candidates.values()]
}

function bridgeCandidatePriority(device: DeviceTarget) {
  if (device.connectionAvailable && device.leaseState === 'active') return 0
  if (device.connectionAvailable) return 1
  if (device.connectionCandidate) return 2
  return 3
}

export type BridgeIdentityValidation = { ok: true } | { ok: false; reason: 'unknown_device' }

export function validateBridgeDeviceIdentity(identity: Identity): BridgeIdentityValidation {
  if (
    !identity.deviceId.trim() ||
    identity.apiVersion !== CONTROL_PLANE_API_VERSION ||
    identity.protocolVersion !== USB_PROTOCOL_VERSION ||
    !REQUIRED_BRIDGE_CAPABILITIES.every((capability) => identity.capabilities.includes(capability))
  ) {
    return { ok: false, reason: 'unknown_device' }
  }

  return { ok: true }
}

export function bridgeProbeToDeviceTarget(
  candidate: DeviceTarget,
  probe: { identity: Identity; network: NetworkSummary; status: ControlPlaneStatus }
): DeviceTarget {
  return {
    ...candidate,
    identityId: probe.identity.deviceId,
    alias: probe.identity.hostname.trim() || probe.identity.deviceId,
    firmware: probe.identity.firmwareVersion,
    buildId: probe.identity.buildId,
    capabilities: probe.identity.capabilities,
    connectionAvailable: true,
    connectionCandidate: false,
    severity: 'nominal',
    leaseState: 'none',
    networkState: probe.network.state,
    wifiSsid: probe.network.ssid ?? null,
    wifiRssi: probe.network.wifiRssi ?? null,
    currentTempC: probe.status.currentTempC,
    targetTempC: probe.status.targetTempC,
    boardTempC: probe.status.boardTempCenti / 100,
    voltageMv: probe.status.voltageMv,
    currentMa: probe.status.currentMa,
    transportIssue: undefined,
  }
}
