import type { DeviceTarget } from './types'

export type DeviceConnectionKind = 'wifi' | 'web-serial' | 'bridge' | 'mock'

export interface DeviceConnectionOption {
  key: string
  kind: DeviceConnectionKind
  label: string
  detail: string
  target: DeviceTarget
}

export interface DeviceChoice {
  identityId: string
  name: string
  connections: DeviceConnectionOption[]
  primary: DeviceTarget
}

export function deviceIdentityId(device: Pick<DeviceTarget, 'id' | 'identityId'>) {
  return device.identityId?.trim() || stripTransportId(device.id)
}

export function isDeviceConnectionAvailable(device: Pick<DeviceTarget, 'connectionAvailable'>) {
  return device.connectionAvailable !== false
}

export function deviceConnectionOptions(
  device: DeviceTarget,
  { allowDemoControls = true }: { allowDemoControls?: boolean } = {}
) {
  if (!isDeviceConnectionAvailable(device)) {
    return []
  }

  const kind = connectionKind(device)
  if (kind === 'mock' && !allowDemoControls) {
    return []
  }

  const labels: Record<DeviceConnectionKind, string> = {
    wifi: 'WiFi / LAN',
    'web-serial': 'Web Serial',
    bridge: '桥接',
    mock: '模拟',
  }

  const bridgeSource =
    device.transport === 'devd' || device.transport === 'bridge'
      ? device.bridgeTransport === 'wifi'
        ? 'WiFi / LAN'
        : 'USB'
      : undefined

  return [
    {
      key: `${deviceIdentityId(device)}:${kind}`,
      kind,
      label: labels[kind],
      detail: bridgeSource ? `${bridgeSource} · ${device.location}` : device.location,
      target: device,
    },
  ] satisfies DeviceConnectionOption[]
}

export function mergeDeviceChoices(
  devices: DeviceTarget[],
  { allowDemoControls = true }: { allowDemoControls?: boolean } = {}
) {
  const choices = new Map<string, DeviceChoice>()

  for (const device of devices) {
    const connections = deviceConnectionOptions(device, { allowDemoControls })
    if (connections.length === 0) continue

    const identityId = deviceIdentityId(device)
    const current = choices.get(identityId)
    if (!current) {
      choices.set(identityId, {
        identityId,
        name: device.alias,
        connections,
        primary: device,
      })
      continue
    }

    for (const connection of connections) {
      const existingIndex = current.connections.findIndex(
        (candidate) => candidate.kind === connection.kind
      )
      if (existingIndex < 0) {
        current.connections.push(connection)
        continue
      }

      const existing = current.connections[existingIndex]
      if (devicePriority(connection.target) < devicePriority(existing.target)) {
        current.connections[existingIndex] = connection
      }
    }
    if (devicePriority(device) < devicePriority(current.primary)) {
      current.primary = device
    }
    if (device.alias.length > current.name.length || current.name === identityId) {
      current.name = device.alias
    }
  }

  return Array.from(choices.values()).map((choice) => ({
    ...choice,
    connections: [...choice.connections].sort(
      (left, right) => connectionOrder(left.kind) - connectionOrder(right.kind)
    ),
  }))
}

function connectionKind(device: DeviceTarget): DeviceConnectionKind {
  if (device.transport === 'wifi' || device.transport === 'http') return 'wifi'
  if (device.transport === 'serial') return 'web-serial'
  if (device.transport === 'devd' || device.transport === 'bridge') return 'bridge'
  if (device.transport === 'mock') return 'mock'
  return 'bridge'
}

function connectionOrder(kind: DeviceConnectionKind) {
  if (kind === 'wifi') return 0
  if (kind === 'web-serial') return 1
  if (kind === 'bridge') return 2
  return 3
}

function stripTransportId(id: string) {
  return id.replace(/^(?:lan|web-serial|native|devd|serial|wifi|mock)-/, '')
}

function devicePriority(device: DeviceTarget) {
  if (device.severity === 'nominal' && device.leaseState === 'active') return 0
  if (device.severity === 'nominal') return 1
  if (device.severity === 'warning') return 2
  return 3
}
