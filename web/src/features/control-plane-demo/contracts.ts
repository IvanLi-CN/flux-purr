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
export type NetworkFailureCode =
  | 'disconnect_timed_out'
  | 'configuration_failed'
  | 'association_rejected'
  | 'association_timed_out'
  | 'ipv4_timed_out'
  | 'station_disconnected'
  | 'lan_startup_failed'
export type PdState = 'negotiating' | 'ready' | 'fallback_5v' | 'fault'
export type FanDisplayState = 'OFF' | 'AUTO' | 'RUN'
export type HeaterLockReason = 'cooling-disabled-overtemp' | 'hard-overtemp'

export interface ThermalTuningTraceCapability {
  paged: boolean
  acknowledged: boolean
  sealedReview: boolean
  bufferCapacity: number
}

export interface ThermalTuningCapability {
  id: 'thermal_tuning_run_v1' | string
  evidenceSchema: 'thermal_tuning_evidence_v3' | string
  supportedPowerClasses: ThermalTuningPowerClass[]
  targetScheduleC: number[]
  physicalTargetsC: number[]
  trace: ThermalTuningTraceCapability
  candidatePromotion: boolean
}

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
  thermalTuning?: ThermalTuningCapability | null
}

export interface NetworkSummary {
  state: NetworkState
  configurationGeneration?: number
  transitionSequence?: number
  failureCode?: NetworkFailureCode | null
  ssid?: string | null
  wifiPasswordLength?: number
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
  rtdRawAdcMv?: number
  vinRawAdcMv?: number
  adcDiagnostics?: AdcDiagnostics
  pdRequestMv: number
  pdContractMv: number
  pdState: PdState
  pdController?: 'ch224q' | 'fusb302b' | 'unknown' | null
  pdContractKind?: 'fixed' | 'pps' | 'none' | null
  pdContractCurrentMa?: number | null
  pdContractPowerMw?: number | null
  pdPerformanceGuaranteed?: boolean | null
  pdDegradedReason?: string | null
  manualPpsEnabled?: boolean
  manualPpsMv?: number | null
  manualPpsMa?: number | null
  ppsCapabilityMinMv?: number | null
  ppsCapabilityMaxMv?: number | null
  ppsCapabilityMaxMa?: number | null
  manualPpsError?: string | null
  faultAttentionPending?: boolean
  heaterLockReason?: HeaterLockReason | null
  calibration: CalibrationRuntimeState
  frontpanelKey?: 'center' | 'right' | 'down' | 'left' | 'up' | null
  network: NetworkSummary
}

export interface AdcDiagnostics {
  calibrationSource: 'efuse' | 'runtime_fallback' | 'unavailable'
  efuseVersion: number
  attenuationDb: number
  initCode?: number | null
  referenceCode?: number | null
  referenceMv?: number | null
  rtdRawCodeMean: number
  rtdRawCodeMin: number
  rtdRawCodeMax: number
  rtdRawCodeSpread: number
  vinRawCodeMean: number
}

export interface InstallStatus {
  layoutId: string
  layoutVersion: number
  partitionTableSha256: string
  persistenceSource: string
  recordState: string
  recordSequence: number
  commissioningRequired: boolean
  setupReason?: string | null
  sensorState: string
  heaterLocked: boolean
}

export type CalibrationMode = 'off' | 'vin_adc' | 'rtd_adc' | 'heater_curve' | 'thermal_plant'
export type CalibrationJobKind = 'vin_adc_auto' | 'thermal_plant_auto'
export type CalibrationJobStatus = 'idle' | 'running' | 'completed' | 'failed' | 'canceled'
export type CalibrationJobOp = 'start' | 'cancel'

export interface CalibrationJobState {
  kind?: CalibrationJobKind | null
  status: CalibrationJobStatus
  progressPercent: number
  samplesCollected: number
  nextRequestMv?: number | null
  message?: string | null
}

export interface CalibrationRuntimeState {
  mode: CalibrationMode
  ppsEnabled: boolean
  ppsMv?: number | null
  ppsMa?: number | null
  heaterEnabled: boolean
  targetAdcMv?: number | null
  stable: boolean
  stabilityErrorMv?: number | null
  error?: string | null
  job: CalibrationJobState
}

export type ThermalPlantRunPhase = 'ambient' | 'heating' | 'cooling'

export interface ThermalPlantTracePoint {
  sampleIndex: number
  elapsedMs: number
  temperatureCentiC: number
  heaterVoltageMv: number
  dutyPercent: number
  phase: ThermalPlantRunPhase
}

export interface ThermalPlantTracePage {
  startSample: number
  nextSample?: number | null
  totalSamples: number
  points: ThermalPlantTracePoint[]
}

export interface ThermalPlantProvisionalCurve {
  state: string
  coveragePercent: number
  curve: HeaterCurvePackage
}

export interface ThermalPlantRunAttempt {
  runId: number
  status: CalibrationJobStatus
  phase?: ThermalPlantRunPhase | null
  progressPercent: number
  elapsedMs: number
  currentTempCentiC: number
  heaterVoltageMv: number
  dutyPercent: number
  sampleCount: number
  restartAllowed: boolean
  error?: string | null
}

export interface ThermalPlantActiveResult {
  transactionId: number
  curve: HeaterCurvePackage
  convectionMwPerC?: number | null
  radiationMwPerK4?: number | null
  thermalCapacityMjPerC?: number | null
  transportDelayMs?: number | null
}

export interface ThermalPlantRunSnapshot {
  version: number
  attempt?: ThermalPlantRunAttempt | null
  tracePage: ThermalPlantTracePage
  provisionalCurve?: ThermalPlantProvisionalCurve | null
  activeResult?: ThermalPlantActiveResult | null
}

export type ThermalTuningPowerClass = 'pps3a' | 'pps5a'
export type ThermalTuningRunOp =
  | 'get'
  | 'start'
  | 'cancel'
  | 'ack_trace'
  | 'seal_review'
  | 'preview'
  | 'discard_preview'
  | 'save'
export type ThermalTuningRunState = 'idle' | 'running' | 'terminal'
export type ThermalTuningPhase =
  | 'idle'
  | 'cooldown_wait'
  | 'scout'
  | 'retune'
  | 'hold_confirm'
  | 'terminal'
export type ThermalTuningReviewState =
  | 'not_applicable'
  | 'recording'
  | 'awaiting_seal'
  | 'complete'
  | 'incomplete'
export type ThermalTuningPromotionState =
  | 'unavailable'
  | 'awaiting_review'
  | 'ready'
  | 'previewed'
  | 'saved'
  | 'expired'
export type ThermalTuningTraceKind =
  | 'sample'
  | 'phase_transition'
  | 'candidate_trial'
  | 'decision'
  | 'safety'
export type ThermalTuningTargetDisposition = 'pending' | 'accepted' | 'failed' | 'skipped'

export interface ThermalTuningEligibility {
  ready: boolean
  reasons: string[]
  activeOwner?: string | null
}

export interface ThermalTuningTargetProgress {
  acceptedC: number[]
  failedC: number[]
  skippedC: number[]
}

export interface ThermalTuningReview {
  state: ThermalTuningReviewState
  reason?: string | null
  acknowledgedThrough?: number | null
  terminalSequence?: number | null
  traceDigest?: string | null
}

export interface ThermalTuningCandidate {
  candidateId?: string | null
  candidateHash?: string | null
  canonicalProfileHex?: string | null
  powerClass?: ThermalTuningPowerClass | null
  promotionState: ThermalTuningPromotionState
}

export interface ThermalTuningJournal {
  lastRunId?: string | null
  lastDisposition?: string | null
  resetReason?: string | null
}

export interface ThermalTuningRun {
  runId: string
  state: ThermalTuningRunState
  powerClass?: ThermalTuningPowerClass | null
  phase: ThermalTuningPhase
  currentTargetC?: number | null
  targetProgress: ThermalTuningTargetProgress
  terminalDisposition?: string | null
  eligibility: ThermalTuningEligibility
  review: ThermalTuningReview
  candidate: ThermalTuningCandidate
  journal: ThermalTuningJournal
}

export interface ThermalTuningTraceEvent {
  sequence: number
  elapsedMs: number
  kind: ThermalTuningTraceKind
  phase?: ThermalTuningPhase | null
  previousPhase?: ThermalTuningPhase | null
  targetC?: number | null
  trialIndex?: number | null
  candidateId?: string | null
  canonicalCandidatePointHex?: string | null
  temperatureCentiC?: number | null
  vinMv?: number | null
  ppsContractMv?: number | null
  ppsContractMa?: number | null
  heaterOutputPermille?: number | null
  heaterPhase?: 'warmup' | 'approach' | 'hold' | null
  measurementValid?: boolean | null
  disposition?: ThermalTuningTargetDisposition | null
  scoreTracking?: number | null
  scoreEnergy?: number | null
  scoreOvershoot?: number | null
  scoreStability?: number | null
  scoreSettleMs?: number | null
  scoreHoldMeanAbsoluteErrorCenti?: number | null
  scoreOutputSwitches?: number | null
  intervalLowerBoundaryC?: number | null
  intervalUpperBoundaryC?: number | null
  intervalPruned?: boolean | null
  candidateFrozen?: boolean | null
  gates?: number | null
  candidateHash?: string | null
  eventReason?: string | null
  trialStartSequence?: number | null
  trialEndSequence?: number | null
  trialStartElapsedMs?: number | null
  trialEndElapsedMs?: number | null
}

export interface ThermalTuningTracePage {
  earliestSequence: number
  emittedThrough?: number | null
  nextAfterSequence: number
  acknowledgedThrough?: number | null
  digestThroughPage?: string | null
  events: ThermalTuningTraceEvent[]
}

export interface ThermalTuningRunSnapshot {
  schema: 'thermal_tuning_run_v1' | string
  run: ThermalTuningRun
  page: ThermalTuningTracePage
  hostPromotionReceipts?: ThermalTuningPromotionReceipt[]
}

export interface ThermalTuningPromotionReceipt {
  recordedAtUnixMs: number
  operation: 'preview' | 'discard_preview' | 'save'
  runId: string
  candidateId?: string | null
  candidateHash?: string | null
  powerClass?: ThermalTuningPowerClass | null
  outcome: 'device_confirmed'
  persistentRevision?: number | null
}

export interface ThermalTuningRunRequest {
  leaseId: string
  op: ThermalTuningRunOp
  runId?: string
  powerClass?: ThermalTuningPowerClass
  afterSequence?: number
  limit?: number
  throughSequence?: number
  traceDigest?: string
  candidateId?: string
  candidateHash?: string
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
  transport: 'mock' | 'native_serial' | 'lan'
  connection: 'disconnected' | 'connected' | 'busy' | 'error'
  identity: Identity
  network: NetworkSummary
  status: ControlPlaneStatus
  calibration?: CalibrationState
  heaterCurve?: HeaterCurveState
  thermalPlantRun?: ThermalPlantRunSnapshot
  thermalTuningRun?: ThermalTuningRunSnapshot
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

export interface DevdLanDeviceSummary {
  id: string
  baseUrl: string
  hostname?: string | null
  lastIpv4?: string | null
  paired: boolean
}

export interface DevdLanDeviceList {
  devices: DevdLanDeviceSummary[]
  discovery?: string
  source?: string
}

export interface DevdLease {
  leaseId: string
  deviceId: string
  ttlMs: number
}

export interface WifiConfigRequest {
  leaseId: string
  op: 'set' | 'clear' | 'cancel'
  ssid?: string
  password?: string
  telemetryIntervalMs?: number
}

export interface WifiConfigReceipt {
  network: NetworkSummary
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
  faultAttentionAcknowledged?: boolean
  calibration?: CalibrationControlRequest
}

export interface CalibrationControlRequest {
  mode?: CalibrationMode
  ppsEnabled?: boolean
  ppsMv?: number
  heaterEnabled?: boolean
  targetAdcMv?: number
}

export type DirectRuntimeConfigRequest = Omit<RuntimeConfigRequest, 'leaseId'>

export type CalibrationChannel = 'rtd_adc' | 'vin_adc'
export type CalibrationSlotId = 'a' | 'b'

export interface BaseCalibrationSample {
  observedMv: number
  expectedMv: number
}

export interface RtdCalibrationSample extends BaseCalibrationSample {
  referenceTempC?: number
  targetAdcMv?: number
}

export interface VinCalibrationSample extends BaseCalibrationSample {
  referenceVinMv?: number
}

export interface CalibrationFit {
  gain: number
  offsetMv: number
  sampleCount: number
}

export interface CalibrationSlotFit {
  gain: number
  offsetMv: number
}

export interface CalibrationSlotSet {
  a: CalibrationSlotFit
  b: CalibrationSlotFit
}

export interface CalibrationChannelState {
  samples: Array<RtdCalibrationSample | VinCalibrationSample | null>
  fittedFit: CalibrationFit
  slots: CalibrationSlotSet
  activeSlot: CalibrationSlotId
}

export interface CalibrationState {
  rtdAdc: CalibrationChannelState
  vinAdc: CalibrationChannelState
}

export interface CalibrationConfigRequest {
  leaseId: string
  op: 'capture' | 'delete' | 'clear' | 'import' | 'set_active_slot' | 'set_slot_fit'
  channel?: CalibrationChannel
  referenceTempC?: number
  referenceVinMv?: number
  targetAdcMv?: number
  observedMv?: number
  expectedMv?: number
  sampleIndex?: number
  state?: CalibrationState
  slot?: CalibrationSlotId
  fit?: CalibrationSlotFit
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

export interface CalibrationJobRequest {
  leaseId: string
  op: CalibrationJobOp
  kind?: CalibrationJobKind
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
  op:
    | 'get_identity'
    | 'get_install_status'
    | 'get_network'
    | 'get_status'
    | 'get_calibration'
    | 'get_calibration_job'
    | 'get_heater_curve'
    | 'set_log_level'
}

export interface UsbWifiConfigFrame {
  type: 'wifi_config'
  requestId: string
  op: 'set' | 'clear' | 'cancel'
  ssid?: string
  password?: string
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
  calibration?: CalibrationControlRequest
}

export interface UsbCalibrationJobFrame {
  type: 'calibration_job'
  requestId: string
  op: CalibrationJobOp
  kind?: CalibrationJobKind
}

export interface UsbThermalPlantRunFrame {
  type: 'thermal_plant_run'
  requestId: string
  afterSample?: number
}

export interface UsbThermalTuningRunFrame {
  type: 'thermal_tuning_run'
  requestId: string
  op: ThermalTuningRunOp
  runId?: string
  powerClass?: ThermalTuningPowerClass
  afterSequence?: number
  limit?: number
  throughSequence?: number
  traceDigest?: string
  candidateId?: string
  candidateHash?: string
}

export interface UsbCalibrationConfigFrame {
  type: 'calibration_config'
  requestId: string
  op: 'capture' | 'delete' | 'clear' | 'import' | 'set_active_slot' | 'set_slot_fit'
  channel?: CalibrationChannel
  referenceTempC?: number
  referenceVinMv?: number
  targetAdcMv?: number
  observedMv?: number
  expectedMv?: number
  sampleIndex?: number
  state?: CalibrationState
  slot?: CalibrationSlotId
  fit?: CalibrationSlotFit
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
