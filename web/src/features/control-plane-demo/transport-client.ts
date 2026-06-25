import type {
  ApiErrorEnvelope,
  ArtifactVerifyResult,
  CalibrationConfigRequest,
  CalibrationJobRequest,
  CalibrationJobState,
  CalibrationState,
  ControlPlaneStatus,
  DevdDeviceList,
  DevdDeviceRecord,
  DevdEvent,
  DevdLease,
  FirmwareArtifactCatalog,
  FirmwareArtifactManifest,
  FlashRequest,
  FlashResult,
  HeaterCurveConfigRequest,
  HeaterCurveState,
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
  getCalibration(devdBaseUrl: string, deviceId: string, leaseId: string): Promise<CalibrationState>
  getCalibrationJob(
    devdBaseUrl: string,
    deviceId: string,
    leaseId: string
  ): Promise<CalibrationJobState>
  configureCalibration(
    devdBaseUrl: string,
    deviceId: string,
    request: CalibrationConfigRequest
  ): Promise<CalibrationState>
  configureCalibrationJob(
    devdBaseUrl: string,
    deviceId: string,
    request: CalibrationJobRequest
  ): Promise<CalibrationJobState>
  applyCalibration(
    devdBaseUrl: string,
    deviceId: string,
    request: { leaseId: string }
  ): Promise<CalibrationState>
  getHeaterCurve(devdBaseUrl: string, deviceId: string, leaseId: string): Promise<HeaterCurveState>
  configureHeaterCurve(
    devdBaseUrl: string,
    deviceId: string,
    request: HeaterCurveConfigRequest
  ): Promise<HeaterCurveState>
  saveHeaterCurve(
    devdBaseUrl: string,
    deviceId: string,
    request: { leaseId: string }
  ): Promise<HeaterCurveState>
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
    getCalibration(devdBaseUrl, deviceId, leaseId) {
      return requestJson<CalibrationState>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/calibration?lease_id=${encodeURIComponent(leaseId)}`
      )
    },
    getCalibrationJob(devdBaseUrl, deviceId, leaseId) {
      return requestJson<CalibrationJobState>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/calibration/job?lease_id=${encodeURIComponent(leaseId)}`
      )
    },
    configureCalibration(devdBaseUrl, deviceId, request) {
      return requestJson<CalibrationState>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/calibration`,
        {
          method: 'PUT',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(request),
        }
      )
    },
    configureCalibrationJob(devdBaseUrl, deviceId, request) {
      return requestJson<CalibrationJobState>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/calibration/job`,
        {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(request),
        }
      )
    },
    applyCalibration(devdBaseUrl, deviceId, request) {
      return requestJson<CalibrationState>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/calibration/apply`,
        {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(request),
        }
      )
    },
    getHeaterCurve(devdBaseUrl, deviceId, leaseId) {
      return requestJson<HeaterCurveState>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/heater-curve?lease_id=${encodeURIComponent(leaseId)}`
      )
    },
    configureHeaterCurve(devdBaseUrl, deviceId, request) {
      return requestJson<HeaterCurveState>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/heater-curve`,
        {
          method: 'PUT',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(request),
        }
      )
    },
    saveHeaterCurve(devdBaseUrl, deviceId, request) {
      return requestJson<HeaterCurveState>(
        fetcher,
        `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/heater-curve/save`,
        {
          method: 'POST',
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
  const transportIssue = devdTransportIssue(record)
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
    rtdRawAdcMv: record.status.rtdRawAdcMv,
    vinRawAdcMv: record.status.vinRawAdcMv,
    voltageMv: record.status.voltageMv,
    currentMa: record.status.currentMa,
    pdRequestMv: record.status.pdRequestMv,
    pdContractMv: record.status.pdContractMv,
    pdState: record.status.pdState,
    manualPpsEnabled: record.status.manualPpsEnabled ?? false,
    manualPpsMv: record.status.manualPpsMv ?? null,
    manualPpsMa: record.status.manualPpsMa ?? null,
    ppsCapabilityMinMv: record.status.ppsCapabilityMinMv ?? null,
    ppsCapabilityMaxMv: record.status.ppsCapabilityMaxMv ?? null,
    ppsCapabilityMaxMa: record.status.ppsCapabilityMaxMa ?? null,
    manualPpsError: record.status.manualPpsError ?? null,
    heaterLockReason: record.status.heaterLockReason ?? null,
    calibration: record.status.calibration,
    storedCalibration: record.calibration,
    heaterEnabled: record.status.heaterEnabled,
    heaterOutputPercent: record.status.heaterOutputPercent,
    activeCoolingEnabled: record.status.activeCoolingEnabled,
    fanState: record.status.fanDisplayState,
    wifiRssi: record.network.wifiRssi ?? null,
    capabilities: record.identity.capabilities,
    networkState: record.network.state,
    leaseState: record.transport === 'native_serial' ? 'none' : undefined,
    transportIssue,
    heaterCurve: record.heaterCurve,
  }
}

function devdTransportIssue(record: DevdDeviceRecord) {
  if (record.transport !== 'native_serial') {
    return undefined
  }

  const lastError = normalizeDevdTransportIssue(record.network.lastError)
  if (lastError) {
    return lastError
  }

  const serialEvent = selectLatestDevdTransportIssueEvent(record.events ?? [])
  if (serialEvent) {
    return devdEventToTransportIssue(serialEvent) ?? devdEventToLogEntry(serialEvent).message
  }

  return undefined
}

export function selectLatestDevdTransportIssueEvent(events: DevdEvent[]) {
  return [...events].reverse().find(isMeaningfulDevdTransportIssueEvent)
}

export function isMeaningfulDevdTransportIssueEvent(event: DevdEvent) {
  if (event.kind !== 'serial') {
    return false
  }

  if (event.message === 'authorized serial port missing') {
    return true
  }

  if (event.message === 'native serial RPC failed') {
    return true
  }

  if (event.payload?.code !== 'firmware_log') {
    return false
  }

  const line = safeString(event.payload?.line)
  if (!line) {
    return false
  }

  return /rst:|panic|abort|brownout|Guru Meditation/i.test(line)
}

export function devdEventToTransportIssue(event: DevdEvent) {
  if (event.kind !== 'serial') {
    return null
  }

  const code = safeString(event.payload?.code)
  if (code === 'authorized_port_missing') {
    const portPath = safeString(event.payload?.portPath)
    const candidates = Array.isArray(event.payload?.candidates)
      ? event.payload?.candidates.filter((value): value is string => typeof value === 'string')
      : []
    if (!portPath) {
      return '已授权串口当前缺失，页面不会自动切换到其它设备。'
    }
    return candidates.length > 0
      ? `已授权串口 ${portPath} 当前缺失；检测到其它 Espressif 端口 ${candidates.join(', ')}，页面不会自动切换。`
      : `已授权串口 ${portPath} 当前缺失，页面不会自动切换到其它设备。`
  }

  if (event.message === 'native serial RPC failed') {
    return normalizeDevdTransportIssue(safeString(event.payload?.message), code)
  }

  const line = safeString(event.payload?.line)
  if (!line) {
    return null
  }
  if (/rst:/i.test(line)) {
    return `串口日志检测到设备复位：${line}`
  }
  if (/panic|abort|brownout|Guru Meditation/i.test(line)) {
    return `串口日志检测到固件异常：${line}`
  }
  return null
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
  if (event.kind === 'serial') {
    return [
      safeString(event.payload?.line),
      safeString(event.payload?.stage),
      safeString(event.payload?.code),
    ]
      .filter(Boolean)
      .join(' / ')
  }
  if (event.kind === 'wifi') {
    return [safeString(event.payload?.ssid), passwordPresence(event.payload?.passwordPresent)]
      .filter(Boolean)
      .join(' / ')
  }
  if (event.kind === 'runtime') {
    const status = recordPayload(event.payload?.status) as Partial<ControlPlaneStatus> | undefined
    return [
      targetTempLabel(status?.targetTempC),
      presetSlotLabel(status?.selectedPresetSlot),
      boolLabel('cooling', status?.activeCoolingEnabled),
      boolLabel('heater', status?.heaterEnabled),
      manualPpsLabel(status?.manualPpsEnabled, status?.manualPpsMv, status?.manualPpsMa),
      calibrationModeLabel(status?.calibration?.mode),
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
  return summarizeTransportFrame(event.payload)
}

function summarizeTransportFrame(payload: DevdEvent['payload']) {
  if (!payload) {
    return undefined
  }

  const frame = payload.frame
  const direction = safeString(payload.direction)?.toUpperCase()
  const frameType = safeString(payload.frameType)
  const requestId = safeString(payload.requestId)
  const status = transportFrameStatus(frame)
  const targetTemp = transportFrameTargetTemp(frame)
  const mode = transportFrameMode(frame)
  const errorCode = transportFrameErrorCode(frame)
  const pieces = [direction, frameType, requestId, status, targetTemp, mode, errorCode].filter(
    Boolean
  )

  return pieces.length > 0 ? pieces.join(' / ') : undefined
}

function transportFrameStatus(frame: unknown) {
  const record = recordPayload(frame)
  if (!record) {
    return null
  }

  if (record.ok === true) {
    return 'ok'
  }
  if (record.ok === false) {
    return 'error'
  }
  return safeString(record.type)
}

function transportFrameTargetTemp(frame: unknown) {
  const record = recordPayload(frame)
  if (!record) {
    return null
  }

  const directTarget = safeNumber(record.targetTempC)
  if (directTarget !== null) {
    return `target ${directTarget}C`
  }

  const result = recordPayload(record.result)
  const status = result ? recordPayload(result.status) : null
  const nestedTarget = status ? safeNumber(status.targetTempC) : null
  return nestedTarget === null ? null : `target ${nestedTarget}C`
}

function transportFrameMode(frame: unknown) {
  const record = recordPayload(frame)
  if (!record) {
    return null
  }

  const directMode = safeString(record.mode)
  if (directMode) {
    return `mode ${directMode}`
  }

  const result = recordPayload(record.result)
  const status = result ? recordPayload(result.status) : null
  const nestedMode = status ? safeString(status.mode) : null
  return nestedMode ? `mode ${nestedMode}` : null
}

function transportFrameErrorCode(frame: unknown) {
  const record = recordPayload(frame)
  if (!record) {
    return null
  }

  const error = recordPayload(record.error)
  const code = error ? safeString(error.code) : null
  return code ? `error ${code}` : null
}

function safeString(value: unknown) {
  return typeof value === 'string' && value.length > 0 ? value : null
}

function normalizeDevdTransportIssue(value: unknown, errorCode?: string | null): string | null {
  const message = safeString(value)

  if (
    errorCode === 'usb_response_timeout' ||
    message === 'Timed out waiting for a matching USB JSONL response.'
  ) {
    return '授权串口在 12 秒内未返回匹配的 USB JSONL 响应；设备可能正在启动、重启，或链路暂时不稳定。'
  }

  if (
    errorCode === 'serial_lock_timeout' ||
    message === 'Timed out waiting for exclusive USB serial access.'
  ) {
    return '授权串口当前被其他进程持续占用；devd 在 8 秒内未拿到独占访问。请关闭其它 devd、串口监视器或终端后重试。'
  }

  if (message?.includes('Resource busy')) {
    return '授权串口当前被其他进程占用（Resource busy）；请关闭其它 devd、串口监视器或终端后重试。'
  }

  if (
    errorCode === 'serial_open_failed' &&
    message?.includes('No such file or directory') &&
    !message.startsWith('Authorized serial port ')
  ) {
    return '已授权串口当前不可用；设备可能刚重枚举，页面不会自动切换到新的串口路径。'
  }

  return message
}

function safeNumber(value: unknown) {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function targetTempLabel(value: unknown) {
  const targetTempC = safeNumber(value)
  return targetTempC === null ? null : `target ${targetTempC}C`
}

function manualPpsLabel(enabled: unknown, millivolts: unknown, _milliamps: unknown) {
  if (enabled !== true) {
    return enabled === false ? 'manual PPS off' : null
  }
  const value = safeNumber(millivolts)
  if (value === null) {
    return 'manual PPS on'
  }
  return `manual PPS ${(value / 1000).toFixed(1)}V`
}

function calibrationModeLabel(mode: unknown) {
  return typeof mode === 'string' && mode !== 'off' ? `calibration ${mode}` : null
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

export function bestEffortReleaseDevdLease(
  devdBaseUrl: string,
  leaseId: string,
  fetcher: typeof fetch = fetch
) {
  try {
    void fetcher(`${devdBaseUrl}/api/v1/leases/${encodeURIComponent(leaseId)}`, {
      method: 'DELETE',
      keepalive: true,
    }).catch(() => undefined)
  } catch {
    // Ignore best-effort release failures during page teardown.
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
