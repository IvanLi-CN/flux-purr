import type {
  ApiErrorEnvelope,
  ArtifactVerifyResult,
  ControlPlaneStatus,
  DevdDeviceList,
  DevdDeviceRecord,
  DevdEvent,
  DevdLease,
  FirmwareArtifactCatalog,
  FirmwareArtifactManifest,
  FlashRequest,
  FlashResult,
  Identity,
  NetworkSummary,
  RuntimeConfigRequest,
  UsbRequestFrame,
  UsbWifiConfigFrame,
  WifiConfigRequest,
} from './contracts'
import type { DeviceTarget, EventLogEntry, FirmwareArtifact } from './types'

export class ControlPlaneClientError extends Error {
  readonly code: string
  readonly retryable: boolean
  readonly details?: unknown

  constructor(message: string, code = 'request_failed', retryable = true, details?: unknown) {
    super(message)
    this.name = 'ControlPlaneClientError'
    this.code = code
    this.retryable = retryable
    this.details = details
  }
}

export interface ControlPlaneHttpClient {
  probeDevice(
    baseUrl: string,
    leaseId?: string
  ): Promise<{
    identity: Identity
    network: NetworkSummary
    status: ControlPlaneStatus
  }>
  probeDevdDevice(
    devdBaseUrl: string,
    deviceId: string,
    leaseId: string
  ): Promise<{
    identity: Identity
    network: NetworkSummary
    status: ControlPlaneStatus
  }>
  listDevdDevices(devdBaseUrl: string): Promise<DevdDeviceRecord[]>
  bindDevdDevice(
    devdBaseUrl: string,
    deviceId: string,
    leaseId: string,
    request: { alias?: string }
  ): Promise<DevdDeviceRecord>
  connectDevdDevice(
    devdBaseUrl: string,
    deviceId: string,
    leaseId: string
  ): Promise<DevdDeviceRecord>
  disconnectDevdDevice(
    devdBaseUrl: string,
    deviceId: string,
    leaseId: string
  ): Promise<DevdDeviceRecord>
  createDevdLease(devdBaseUrl: string, deviceId: string): Promise<DevdLease>
  heartbeatDevdLease(devdBaseUrl: string, leaseId: string): Promise<DevdLease>
  releaseDevdLease(devdBaseUrl: string, leaseId: string): Promise<void>
  configureRuntime(
    devdBaseUrl: string,
    deviceId: string,
    request: RuntimeConfigRequest
  ): Promise<ControlPlaneStatus>
  configureWifi(
    devdBaseUrl: string,
    deviceId: string,
    request: WifiConfigRequest
  ): Promise<NetworkSummary>
  listDevdArtifacts(devdBaseUrl: string): Promise<FirmwareArtifact[]>
  verifyArtifact(
    devdBaseUrl: string,
    artifact: FirmwareArtifactManifest
  ): Promise<ArtifactVerifyResult>
  flashDevice(devdBaseUrl: string, deviceId: string, request: FlashRequest): Promise<FlashResult>
}

export function createControlPlaneHttpClient(
  fetcher: typeof fetch = fetch
): ControlPlaneHttpClient {
  return {
    async probeDevice(baseUrl, leaseId) {
      const suffix = leaseId ? `?lease_id=${encodeURIComponent(leaseId)}` : ''
      const [identity, network, status] = await Promise.all([
        requestJson<Identity>(fetcher, `${baseUrl}/api/v1/identity${suffix}`),
        requestJson<NetworkSummary>(fetcher, `${baseUrl}/api/v1/network${suffix}`),
        requestJson<ControlPlaneStatus>(fetcher, `${baseUrl}/api/v1/status${suffix}`),
      ])

      return { identity, network, status }
    },
    async probeDevdDevice(devdBaseUrl, deviceId, leaseId) {
      const encodedDeviceId = encodeURIComponent(deviceId)
      const suffix = `?lease_id=${encodeURIComponent(leaseId)}`
      const identity = await requestJson<Identity>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodedDeviceId}/identity${suffix}`
      )
      const network = await requestJson<NetworkSummary>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodedDeviceId}/network${suffix}`
      )
      const status = await requestJson<ControlPlaneStatus>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodedDeviceId}/status${suffix}`
      )

      return { identity, network, status }
    },
    async listDevdDevices(devdBaseUrl) {
      const response = await requestJson<DevdDeviceList>(fetcher, `${devdBaseUrl}/api/v1/devices`)
      return response.devices
    },
    bindDevdDevice(devdBaseUrl, deviceId, leaseId, request) {
      return requestJson<DevdDeviceRecord>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/bind?lease_id=${encodeURIComponent(leaseId)}`,
        {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(request),
        }
      )
    },
    connectDevdDevice(devdBaseUrl, deviceId, leaseId) {
      return requestJson<DevdDeviceRecord>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/connect?lease_id=${encodeURIComponent(leaseId)}`,
        {
          method: 'POST',
        }
      )
    },
    disconnectDevdDevice(devdBaseUrl, deviceId, leaseId) {
      return requestJson<DevdDeviceRecord>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/disconnect?lease_id=${encodeURIComponent(leaseId)}`,
        {
          method: 'POST',
        }
      )
    },
    createDevdLease(devdBaseUrl, deviceId) {
      return requestJson<DevdLease>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/leases`,
        {
          method: 'POST',
        }
      )
    },
    heartbeatDevdLease(devdBaseUrl, leaseId) {
      return requestJson<DevdLease>(
        fetcher,
        `${devdBaseUrl}/api/v1/leases/${encodeURIComponent(leaseId)}/heartbeat`,
        {
          method: 'POST',
        }
      )
    },
    async releaseDevdLease(devdBaseUrl, leaseId) {
      await requestJson<{ released: boolean }>(
        fetcher,
        `${devdBaseUrl}/api/v1/leases/${encodeURIComponent(leaseId)}`,
        {
          method: 'DELETE',
        }
      )
    },
    configureRuntime(devdBaseUrl, deviceId, request) {
      return requestJson<ControlPlaneStatus>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/runtime`,
        {
          method: 'PUT',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(request),
        }
      )
    },
    async configureWifi(devdBaseUrl, deviceId, request) {
      const response = await requestJson<{ network: NetworkSummary }>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/wifi`,
        {
          method: 'PUT',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(request),
        }
      )
      return response.network
    },
    async listDevdArtifacts(devdBaseUrl) {
      const response = await requestJson<FirmwareArtifactCatalog>(
        fetcher,
        `${devdBaseUrl}/api/v1/artifacts`
      )
      return response.artifacts.map(manifestToArtifact)
    },
    verifyArtifact(devdBaseUrl, artifact) {
      return requestJson<ArtifactVerifyResult>(fetcher, `${devdBaseUrl}/api/v1/artifacts/verify`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ artifact }),
      })
    },
    flashDevice(devdBaseUrl, deviceId, request) {
      return requestJson<FlashResult>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/flash`,
        {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(request),
        }
      )
    },
  }
}

export function devdRecordToDeviceTarget(record: DevdDeviceRecord): DeviceTarget {
  return {
    id: record.id,
    alias: record.displayName,
    location: record.portPath ?? 'localhost devd',
    transport: record.transport === 'native_serial' ? 'devd' : 'mock',
    severity: record.connection === 'connected' ? 'nominal' : 'warning',
    baseUrl: `devd://${record.id}`,
    firmware: record.identity.firmwareVersion,
    buildId: record.identity.buildId,
    uptime: formatUptime(record.status.uptimeSeconds),
    boardTempC: record.status.boardTempCenti / 100,
    currentTempC: record.status.currentTempC,
    targetTempC: record.status.targetTempC,
    selectedPresetIndex: record.status.selectedPresetSlot,
    presetsC: record.status.presetsC,
    voltageMv: record.status.voltageMv,
    currentMa: record.status.currentMa,
    pdRequestMv: record.status.pdRequestMv,
    pdContractMv: record.status.pdContractMv,
    pdState: record.status.pdState,
    heaterOutputPercent: record.status.heaterOutputPercent,
    activeCoolingEnabled: record.status.activeCoolingEnabled,
    fanState: record.status.fanDisplayState,
    wifiRssi: record.network.wifiRssi ?? null,
    capabilities: record.identity.capabilities,
    networkState: record.network.state,
    leaseState: record.transport === 'native_serial' ? 'none' : undefined,
  }
}

export function devdEventToLogEntry(event: DevdEvent): EventLogEntry {
  const detail = devdEventDetail(event)
  return {
    time: event.timestamp,
    source: event.kind,
    message: detail ? `${event.message}: ${detail}` : event.message,
    tone: devdEventTone(event),
    detail: devdEventFrameDetail(event),
  }
}

function devdEventDetail(event: DevdEvent) {
  if (event.kind === 'wifi') {
    return [safeString(event.payload?.ssid), passwordPresence(event.payload?.passwordPresent)]
      .filter(Boolean)
      .join(' / ')
  }
  if (event.kind === 'runtime') {
    const status = recordPayload(event.payload?.status)
    return [
      targetTempLabel(status?.targetTempC),
      presetSlotLabel(status?.selectedPresetSlot),
      boolLabel('cooling', status?.activeCoolingEnabled),
      boolLabel('heater', status?.heaterEnabled),
    ]
      .filter(Boolean)
      .join(' / ')
  }
  if (event.kind === 'flash') {
    return [safeString(event.payload?.artifactId), safeString(event.payload?.code)]
      .filter(Boolean)
      .join(' / ')
  }
  if (event.kind === 'lease') {
    return safeString(event.payload?.leaseId)
  }
  if (event.kind === 'transport') {
    return [
      safeString(event.payload?.direction)?.toUpperCase(),
      safeString(event.payload?.transport),
      safeString(event.payload?.frameType),
      safeString(event.payload?.requestId),
    ]
      .filter(Boolean)
      .join(' / ')
  }

  return [safeString(event.payload?.stage), safeString(event.payload?.code)]
    .filter(Boolean)
    .join(' / ')
}

function devdEventFrameDetail(event: DevdEvent) {
  if (event.kind !== 'transport') {
    return undefined
  }
  const frame = event.payload?.frame
  if (frame == null) {
    return undefined
  }
  return stableStringify(frame)
}

function stableStringify(value: unknown) {
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

function safeString(value: unknown) {
  return typeof value === 'string' && value.length > 0 ? value : null
}

function safeNumber(value: unknown) {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function targetTempLabel(value: unknown) {
  const targetTempC = safeNumber(value)
  return targetTempC === null ? null : `target ${targetTempC}C`
}

function presetSlotLabel(value: unknown) {
  const selectedPresetSlot = safeNumber(value)
  return selectedPresetSlot === null ? null : `preset M${selectedPresetSlot + 1}`
}

function passwordPresence(value: unknown) {
  if (typeof value !== 'boolean') {
    return null
  }
  return value ? 'password present' : 'open network'
}

function recordPayload(value: unknown) {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null
}

function boolLabel(label: string, value: unknown) {
  if (typeof value !== 'boolean') {
    return null
  }
  return `${label} ${value ? 'on' : 'off'}`
}

function devdEventTone(event: DevdEvent): EventLogEntry['tone'] {
  if (event.kind === 'serial') {
    return 'danger'
  }
  if (event.kind === 'transport') {
    return event.payload?.direction === 'rx' ? 'success' : 'info'
  }
  if (event.kind === 'flash' && event.message.toLowerCase().includes('failed')) {
    return 'danger'
  }
  if (event.kind === 'lease') {
    return 'info'
  }
  if (event.kind === 'wifi' || event.kind === 'runtime') {
    return 'success'
  }
  if (event.kind === 'flash') {
    return 'success'
  }
  return 'info'
}

export function artifactToManifest(artifact: FirmwareArtifact): FirmwareArtifactManifest {
  return {
    artifactId: artifact.id,
    name: artifact.version,
    version: artifact.version,
    gitSha: 'unknown',
    buildId: artifact.hash.replace(/^sha256:/, ''),
    targetChip: artifact.target,
    profile: artifact.profile,
    features: artifact.features ?? [],
    protocol: artifact.protocol ?? 'flux-purr.usb.v1',
    files: artifact.files ?? [],
  }
}

export function manifestToArtifact(manifest: FirmwareArtifactManifest): FirmwareArtifact {
  return {
    id: manifest.artifactId,
    version: manifest.version,
    target: manifest.targetChip,
    profile: manifest.profile,
    compatibility: manifest.targetChip === 'esp32s3' ? 'match' : 'blocked',
    hash: manifest.files[0]?.sha256 ?? `sha256:${manifest.buildId}`,
    progressPercent: 0,
    protocol: manifest.protocol,
    features: manifest.features,
    files: manifest.files,
  }
}

export function createUsbRequestFrame(
  requestId: string,
  op: UsbRequestFrame['op']
): UsbRequestFrame {
  return { type: 'request', requestId, op }
}

export function createUsbWifiConfigFrame(
  requestId: string,
  request: Omit<UsbWifiConfigFrame, 'type' | 'requestId'>
): UsbWifiConfigFrame {
  return { type: 'wifi_config', requestId, ...request }
}

export function redactWifiConfigFrame(frame: UsbWifiConfigFrame): UsbWifiConfigFrame {
  return {
    ...frame,
    password: frame.password ? '<redacted>' : undefined,
  }
}

async function requestJson<T>(fetcher: typeof fetch, url: string, init?: RequestInit): Promise<T> {
  const response = await fetcher(url, init)
  const payload = (await response.json().catch(() => null)) as T | ApiErrorEnvelope | null

  if (!response.ok) {
    const envelope = payload as ApiErrorEnvelope | null
    throw new ControlPlaneClientError(
      envelope?.error.message ?? `Request failed with ${response.status}`,
      envelope?.error.code,
      envelope?.error.retryable,
      envelope?.error.details
    )
  }

  return payload as T
}

function formatUptime(seconds: number) {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const rest = seconds % 60
  return [hours, minutes, rest].map((value) => String(value).padStart(2, '0')).join(':')
}
