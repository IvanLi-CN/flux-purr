export const CONTROL_PLANE_API_VERSION = '2026-05-29'
export const USB_PROTOCOL_VERSION = 'flux-purr.usb.v1'

export type TransportKind = 'http' | 'serial' | 'devd' | 'mock'
export type NetworkState =
  | 'disabled'
  | 'idle'
  | 'saving'
  | 'connecting'
  | 'connected'
  | 'error'
  | 'timeout'
export type PdState = 'negotiating' | 'ready' | 'fallback_5v' | 'fault'
export type FanDisplayState = 'OFF' | 'AUTO' | 'RUN'

export interface Identity {
  deviceId: string
  firmwareVersion: string
  buildId: string
  gitSha: string
  board: string
  apiVersion: string
  protocolVersion: string
  hostname: string
  capabilities: string[]
}

export interface NetworkSummary {
  state: NetworkState
  ssid?: string | null
  ip?: string | null
  gateway?: string | null
  dns?: string[]
  wifiRssi?: number | null
  lastError?: string | null
}

export interface ControlPlaneStatus {
  mode: 'idle' | 'sampling' | 'fault'
  uptimeSeconds: number
  currentTempC: number
  targetTempC: number
  selectedPresetSlot?: number
  presetsC?: Array<number | null>
  heaterEnabled: boolean
  heaterOutputPercent: number
  activeCoolingEnabled: boolean
  fanDisplayState: FanDisplayState
  fanEnabled: boolean
  fanPwmPermille: number
  voltageMv: number
  currentMa: number
  boardTempCenti: number
  pdRequestMv: number
  pdContractMv: number
  pdState: PdState
  manualPpsEnabled?: boolean
  manualPpsMv?: number | null
  manualPpsMa?: number | null
  ppsCapabilityMinMv?: number | null
  ppsCapabilityMaxMv?: number | null
  ppsCapabilityMaxMa?: number | null
  manualPpsError?: string | null
  frontpanelKey?: 'center' | 'right' | 'down' | 'left' | 'up' | null
  network: NetworkSummary
}

export interface ApiErrorEnvelope {
  error: {
    code: string
    message: string
    retryable: boolean
    details?: unknown
  }
}

export interface DevdDeviceRecord {
  id: string
  displayName: string
  portPath?: string | null
  transport: 'mock' | 'native_serial'
  connection: 'disconnected' | 'connected' | 'busy' | 'error'
  identity: Identity
  network: NetworkSummary
  status: ControlPlaneStatus
  calibration?: CalibrationState
  logs?: DevdLogEntry[]
  trace?: DevdTraceEntry[]
  events?: DevdEvent[]
}

export interface DevdLogEntry {
  id: string
  timestamp: string
  level: string
  message: string
}

export interface DevdTraceEntry {
  id: string
  timestamp: string
  direction: string
  frameType: string
  requestId?: string | null
  summary: string
  payload: unknown
}

export interface DevdEvent {
  id: string
  timestamp: string
  deviceId?: string | null
  kind: string
  message: string
  payload?:
    | (Record<string, unknown> & {
        stage?: string
        code?: string
        message?: string
        retryable?: boolean
        ssid?: string
        passwordPresent?: boolean
        artifactId?: string
        leaseId?: string
        direction?: string
        transport?: string
        frameType?: string
        requestId?: string
        frame?: unknown
      })
    | null
}

export interface DevdDeviceList {
  devices: DevdDeviceRecord[]
}

export interface DevdLease {
  leaseId: string
  deviceId: string
  ttlMs: number
}

export interface WifiConfigRequest {
  leaseId: string
  op: 'set' | 'clear'
  ssid?: string
  password?: string
  autoReconnect?: boolean
  telemetryIntervalMs?: number
}

export interface RuntimeConfigRequest {
  leaseId: string
  targetTempC?: number
  selectedPresetSlot?: number
  presetsC?: Array<number | null>
  activeCoolingEnabled?: boolean
  heaterEnabled?: boolean
  manualPpsEnabled?: boolean
  manualPpsMv?: number
  manualPpsMa?: number
}

export type DirectRuntimeConfigRequest = Omit<RuntimeConfigRequest, 'leaseId'>

export type CalibrationChannel = 'rtd_adc' | 'vin_adc'

export interface CalibrationSample {
  observedMv: number
  expectedMv: number
}

export interface CalibrationPackage {
  rtdAdc: Array<CalibrationSample | null>
  vinAdc: Array<CalibrationSample | null>
}

export interface CalibrationFit {
  gain: number
  offsetMv: number
  customSampleCount: number
  defaultSampleCount: number
}

export interface CalibrationFits {
  rtdAdc: CalibrationFit
  vinAdc: CalibrationFit
}

export interface CalibrationState {
  active: CalibrationPackage
  draft: CalibrationPackage
  activeFit: CalibrationFits
  draftFit: CalibrationFits
}

export interface CalibrationConfigRequest {
  leaseId: string
  op: 'capture' | 'delete' | 'clear' | 'import'
  channel?: CalibrationChannel
  referenceTempC?: number
  referenceVinMv?: number
  observedMv?: number
  expectedMv?: number
  sampleIndex?: number
  package?: CalibrationPackage
}

export interface HeaterCurvePoint {
  tempCentiC: number
  resistanceMilliohms: number
}

export interface HeaterCurvePackage {
  points: Array<HeaterCurvePoint | null>
}

export interface HeaterCurveState {
  active: HeaterCurvePackage
  preview: HeaterCurvePackage | null
}

export interface HeaterCurveConfigRequest {
  leaseId: string
  op: 'preview' | 'clear_preview'
  package?: HeaterCurvePackage
}

export interface FirmwareArtifactManifest {
  artifactId: string
  name: string
  version: string
  gitSha: string
  buildId: string
  targetChip: string
  profile: string
  features: string[]
  protocol: string
  files: Array<{
    kind: string
    path: string
    sha256: string
    size: number
    flashAddress?: number | null
  }>
}

export interface FirmwareArtifactCatalog {
  artifacts: FirmwareArtifactManifest[]
}

export interface ArtifactVerifyResult {
  artifactId: string
  verified: boolean
  files: Array<{
    kind: string
    sha256: string
    size: number
    ok: boolean
  }>
}

export interface FlashRequest {
  leaseId: string
  artifact: FirmwareArtifactManifest
  dryRun: boolean
  confirm?: 'FLASH'
}

export interface FlashResult {
  artifactId: string
  dryRun: boolean
  status: string
  message: string
}

export interface UsbRequestFrame {
  type: 'request'
  requestId: string
  op: 'get_identity' | 'get_network' | 'get_status' | 'get_heater_curve' | 'set_log_level'
}

export interface UsbWifiConfigFrame {
  type: 'wifi_config'
  requestId: string
  op: 'set' | 'clear'
  ssid?: string
  password?: string
  autoReconnect?: boolean
  telemetryIntervalMs?: number
}

export interface UsbRuntimeConfigFrame {
  type: 'runtime_config'
  requestId: string
  targetTempC?: number
  selectedPresetSlot?: number
  presetsC?: Array<number | null>
  activeCoolingEnabled?: boolean
  heaterEnabled?: boolean
  manualPpsEnabled?: boolean
  manualPpsMv?: number
  manualPpsMa?: number
}

export interface UsbHeaterCurveConfigFrame {
  type: 'heater_curve_config'
  requestId: string
  op: 'preview' | 'clear_preview'
  heaterCurve?: HeaterCurvePackage
}

export interface UsbHeaterCurveSaveFrame {
  type: 'heater_curve_save'
  requestId: string
}
