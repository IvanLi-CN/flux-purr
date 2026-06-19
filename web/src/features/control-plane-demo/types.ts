import type { LucideIcon } from 'lucide-react'
import type { HeaterCurveState } from './contracts'

export type TransportKind = 'http' | 'serial' | 'devd' | 'mock' | 'wifi' | 'bridge'
export type DeviceSeverity = 'nominal' | 'warning' | 'offline'
export type WorkstreamId = 'fleet' | 'connect' | 'overview' | 'wifi' | 'firmware' | 'monitor'

export interface DeviceTarget {
  id: string
  alias: string
  location: string
  transport: TransportKind
  severity: DeviceSeverity
  baseUrl: string
  firmware: string
  buildId: string
  uptime: string
  boardTempC: number
  currentTempC: number
  targetTempC: number
  selectedPresetIndex?: number
  presetsC?: Array<number | null>
  voltageMv: number
  currentMa: number
  pdRequestMv: number
  pdContractMv: number
  pdState: 'negotiating' | 'ready' | 'fallback_5v' | 'fault'
  manualPpsEnabled?: boolean
  manualPpsMv?: number | null
  manualPpsMa?: number | null
  ppsCapabilityMinMv?: number | null
  ppsCapabilityMaxMv?: number | null
  ppsCapabilityMaxMa?: number | null
  manualPpsError?: string | null
  heaterEnabled: boolean
  heaterOutputPercent: number
  activeCoolingEnabled: boolean
  fanState: 'OFF' | 'AUTO' | 'RUN'
  wifiRssi: number | null
  capabilities: string[]
  networkState?: 'disabled' | 'idle' | 'saving' | 'connecting' | 'connected' | 'error' | 'timeout'
  leaseState?: 'none' | 'active' | 'conflict' | 'expired'
  leaseId?: string
  transportIssue?: string
  heaterCurve?: HeaterCurveState
}

export interface ControlPlaneMetric {
  label: string
  value: string
  detail: string
  tone: 'neutral' | 'accent' | 'success' | 'warning'
}

export interface WorkflowPhase {
  label: string
  detail: string
  state: 'done' | 'active' | 'pending' | 'blocked'
}

export interface FirmwareArtifact {
  id: string
  version: string
  target: string
  profile: string
  compatibility: 'match' | 'warning' | 'blocked'
  hash: string
  progressPercent: number
  protocol?: string
  features?: string[]
  files?: Array<{
    kind: string
    path: string
    sha256: string
    size: number
    flashAddress?: number | null
  }>
}

export interface EventLogEntry {
  time: string
  source: string
  message: string
  tone: 'info' | 'success' | 'warning' | 'danger'
  detail?: string
}

export interface Workstream {
  id: WorkstreamId
  label: string
  description: string
  icon: LucideIcon
}

export interface ControlPlaneScenario {
  name: string
  headline: string
  subhead: string
  selectedDeviceId: string
  devices: DeviceTarget[]
  metrics: ControlPlaneMetric[]
  wifiPhases: WorkflowPhase[]
  flashPhases: WorkflowPhase[]
  artifacts: FirmwareArtifact[]
  events: EventLogEntry[]
}
