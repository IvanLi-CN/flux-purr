import type {
  ApiErrorEnvelope,
  ControlPlaneStatus,
  DevdDeviceList,
  DevdDeviceRecord,
  DevdLease,
  FirmwareArtifactManifest,
  Identity,
  NetworkSummary,
  UsbRequestFrame,
  UsbWifiConfigFrame,
  WifiConfigRequest,
} from './contracts'
import type { DeviceTarget, FirmwareArtifact } from './types'

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
  listDevdDevices(devdBaseUrl: string): Promise<DevdDeviceRecord[]>
  createDevdLease(devdBaseUrl: string, deviceId: string): Promise<DevdLease>
  heartbeatDevdLease(devdBaseUrl: string, leaseId: string): Promise<DevdLease>
  configureWifi(
    devdBaseUrl: string,
    deviceId: string,
    request: WifiConfigRequest
  ): Promise<NetworkSummary>
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
    async listDevdDevices(devdBaseUrl) {
      const response = await requestJson<DevdDeviceList>(fetcher, `${devdBaseUrl}/api/v1/devices`)
      return response.devices
    },
    createDevdLease(devdBaseUrl, deviceId) {
      return requestJson<DevdLease>(fetcher, `${devdBaseUrl}/api/v1/devices/${deviceId}/leases`, {
        method: 'POST',
      })
    },
    heartbeatDevdLease(devdBaseUrl, leaseId) {
      return requestJson<DevdLease>(fetcher, `${devdBaseUrl}/api/v1/leases/${leaseId}/heartbeat`, {
        method: 'POST',
      })
    },
    async configureWifi(devdBaseUrl, deviceId, request) {
      const response = await requestJson<{ network: NetworkSummary }>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${deviceId}/wifi`,
        {
          method: 'PUT',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(request),
        }
      )
      return response.network
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
    files: [],
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
