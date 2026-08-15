import { Link } from '@tanstack/react-router'
import { useVirtualizer } from '@tanstack/react-virtual'
import {
  AlertTriangle,
  Cable,
  CheckCircle2,
  ChevronDown,
  CircleHelp,
  Download,
  Fan,
  Gauge,
  LoaderCircle,
  Minus,
  Plus,
  Power,
  RefreshCw,
  Router,
  ScanSearch,
  SlidersHorizontal,
  ToggleRight,
  Trash2,
  Upload,
  Usb,
  Wifi,
  Wrench,
  X,
  Zap,
} from 'lucide-react'
import type { CSSProperties, Dispatch, ReactNode, SetStateAction } from 'react'
import {
  Fragment,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import { createPortal } from 'react-dom'
import SimpleBar from 'simplebar-react'
import type { AppVariant } from '@/app-mode'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import type { ConsoleRouteState } from '@/routing/console-route'
import { readRoutePreferences, rememberSuccessfulRoute } from '@/routing/route-preferences'
import type { AppSearch } from '@/routing/search'
import {
  bridgeCandidatesForTransport,
  bridgeProbeToDeviceTarget,
  validateBridgeDeviceIdentity,
} from '../bridge-device-connection'
import { syncCalibrationDraftText } from '../calibration-draft'
import {
  type CalibrationLeaveRequest,
  type CalibrationWorkspaceTab,
  type ConsoleView,
  shouldBlockCalibrationDeviceChange,
  shouldBlockCalibrationViewChange,
  shouldBlockCalibrationWorkspaceTabChange,
} from '../calibration-leave-guard'
import {
  resolveCalibrationSliderValue,
  validateCalibrationSliderText,
} from '../calibration-slider-value'
import type {
  BaseCalibrationSample,
  CalibrationChannel,
  CalibrationChannelState,
  CalibrationConfigRequest,
  CalibrationControlRequest,
  CalibrationFit,
  CalibrationMode,
  CalibrationRuntimeState,
  CalibrationSlotFit,
  CalibrationSlotId,
  CalibrationState,
  ControlPlaneStatus,
  DevdLanDeviceSummary,
  HeaterCurveConfigRequest,
  HeaterCurvePackage,
  HeaterCurveState,
  NetworkSummary,
  RtdCalibrationSample,
  VinCalibrationSample,
} from '../contracts'
import {
  type DeviceChoice,
  type DeviceConnectionKind,
  type DeviceConnectionOption,
  deviceConnectionOptions,
  deviceIdentityId,
  mergeDeviceChoices,
  preferredDeviceConnection,
} from '../device-target-picker'
import {
  knownWebSerialDeviceToTarget,
  listKnownWebSerialDevices,
  rememberKnownWebSerialDevice,
} from '../known-web-serial-devices'
import {
  authorizedLanRequest,
  createLanLease,
  directLanStaleReadPath,
  getLanPublicInfo,
  isDirectLanDevice,
  type LanDeviceSession,
  type LanLease,
  type LanProbe,
  lanProbeToDeviceTarget,
  listSavedLanDeviceSessions,
  loadLanDeviceSession,
  probeLanDevice,
  reconcileStaleLanWrite,
  releaseLanLease,
  resumeLanDeviceSession,
  savedLanSessionToDeviceTarget,
  startLanLeaseHeartbeat,
  streamLanEvents,
  upsertLanDeviceTarget,
  writeLanRuntime,
} from '../lan-client'
import {
  defaultDevdBaseUrl,
  type LiveDevdOptions,
  shouldHoldDevdLease,
  useLiveDevdScenario,
} from '../live-devd'
import {
  type LiveWebSerialControls,
  type LiveWebSerialOptions,
  useLiveWebSerialScenario,
} from '../live-web-serial'
import { controlPlaneScenario, degradedControlPlaneScenario } from '../mock-data'
import {
  createPendingHeaterFeedback,
  deviceControlBlockReason,
  HEATER_CONFIRMATION_TIMEOUT_MS,
  heaterConfirmationNowMs,
  heaterLockReasonText,
  lanLeaseAcquisitionRequest,
  lanLeaseHeartbeatFailureDetail,
  type PendingHeaterConfirmation,
  resolvePendingHeaterConfirmation,
  runtimeHeaterState,
  shouldReacquireLanLeaseOnExplicitSelection,
  shouldReplacePassiveFeedbackWithHeaterLock,
} from '../runtime-status'
import {
  artifactToManifest,
  ControlPlaneClientError,
  type ControlPlaneHttpClient,
  createControlPlaneHttpClient,
  devdRecordToDeviceTarget,
} from '../transport-client'
import type {
  ControlPlaneScenario,
  DeviceSeverity,
  DeviceTarget,
  EventLogEntry,
  FirmwareArtifact,
  TransportKind,
  WorkflowPhase,
} from '../types'
import { UNAVAILABLE_TEMPERATURE_C } from '../types'
import { isDirectWebSerialDevice } from '../web-serial'
import { LanPairingPanel, type LanPairingPanelProps } from './lan-pairing-panel'
import {
  resolveWifiSettingsUnavailableReason,
  WifiNetworkSettings,
  type WifiNetworkSettingsDraft,
} from './wifi-network-settings'

export interface LanRuntimeDependencies {
  createLease?: typeof createLanLease
  releaseLease?: typeof releaseLanLease
  startLeaseHeartbeat?: typeof startLanLeaseHeartbeat
  streamEvents?: typeof streamLanEvents
  probeDevice?: typeof probeLanDevice
  getPublicInfo?: typeof getLanPublicInfo
  writeRuntime?: typeof writeLanRuntime
  readStatus?: (session: LanDeviceSession) => Promise<ControlPlaneStatus>
  readCalibration?: (session: LanDeviceSession) => Promise<CalibrationState>
  readHeaterCurve?: (session: LanDeviceSession) => Promise<HeaterCurveState>
}

type LanPairingOverrides = Omit<LanPairingPanelProps, 'onPaired'>

export interface BlockedConsoleNavigation {
  next: ConsoleRouteState | null
  nextLabel: string
  proceed: () => void
  reset: () => void
}

export interface CalibrationRouteGuard {
  deviceId: string
  workspaceTab: CalibrationWorkspaceTab
}

export interface ConsoleNavigationAdapter {
  state: ConsoleRouteState
  variant: AppVariant
  search: AppSearch
  navigate: (
    state: ConsoleRouteState,
    options?: { replace?: boolean; ignoreBlocker?: boolean }
  ) => Promise<void>
  blockedNavigation: BlockedConsoleNavigation | null
  onCalibrationGuardChange: (guard: CalibrationRouteGuard | null) => void
}

interface ControlPlaneDemoProps {
  scenario?: ControlPlaneScenario
  initialView?: ConsoleView
  navigation?: ConsoleNavigationAdapter
  devd?: LiveDevdOptions
  webSerial?: LiveWebSerialOptions
  allowDemoControls?: boolean
  lanPairing?: LanPairingOverrides
  lanRuntime?: LanRuntimeDependencies
}
type CalibrationWorkbenchMode = 'vin_adc' | 'rtd_adc' | 'heater_curve'
type FlashRunStatus = 'idle' | 'running' | 'passed' | 'flashing' | 'flashed'
type AddDeviceKind = 'wifi' | 'web-serial' | 'bridge'
type LogFilter = 'all' | EventLogEntry['tone']

const defaultAddDeviceKind: AddDeviceKind = 'wifi'

interface ActionFeedback {
  title: string
  detail: string
  tone: 'info' | 'success' | 'warning'
}

interface CalibrationLeaveGuardState extends CalibrationLeaveRequest {
  continueAction: () => void | Promise<void>
  cancelAction?: () => void
  nextView?: ConsoleView
  nextWorkspaceTab?: CalibrationWorkspaceTab
  anchorId?: string
}

const LOG_FEED_SIZE = 1000
const LOG_FEED_STEP_SECONDS = 3
const LOG_FEED_START_SECONDS = 20 * 3600 + 14 * 60 + 3
function parseCalibrationIntegerInput(rawValue: string) {
  if (rawValue.trim() === '') {
    return null
  }
  const next = Number(rawValue)
  return Number.isFinite(next) ? Math.round(next) : null
}

const LOG_FILTER_OPTIONS: Array<{ value: LogFilter; label: string }> = [
  { value: 'all', label: '全部' },
  { value: 'info', label: '信息' },
  { value: 'success', label: '完成' },
  { value: 'warning', label: '警告' },
  { value: 'danger', label: '错误' },
]
const TARGET_TEMP_MIN = 0
const TARGET_TEMP_MAX = 400
const TARGET_TEMP_STEP = 5

function resumeConnectionPriority(connection: DeviceConnectionOption) {
  if (connection.target.severity === 'nominal' && connection.target.leaseState === 'active')
    return 0
  if (connection.kind === 'wifi') return 1
  if (connection.kind === 'web-serial') return 2
  if (connection.kind === 'bridge') return 3
  return 4
}

function isHealthyRouteConnection(connection: DeviceConnectionOption) {
  return (
    connection.target.connectionAvailable !== false &&
    connection.target.severity === 'nominal' &&
    (connection.kind === 'mock' || connection.target.leaseState === 'active')
  )
}
const PPS_STEP_MV = 100
const PPS_HARDWARE_MIN_MV = 5_000
const PPS_HARDWARE_MAX_MV = 28_000
const RTD_TARGET_MIN_MV = 0
const RTD_TARGET_MAX_MV = 2_800
const RTD_TARGET_STEP_MV = 10
const PRESET_COMMIT_DEBOUNCE_MS = 650
const CALIBRATION_ACTION_LOCK_MS = 800
const LIVE_DEVD_TRANSIENT_DEVICE_IDS = new Set(['live-devd-bootstrapping', 'live-devd-unavailable'])
const PRESET_TEMPS_C = [50, 100, 120, 150, 180, 200, 210, 220, 250, 300]
const PRESETS_C = PRESET_TEMPS_C.map((tempC) => tempC as number | null)
const PRESET_ENABLED = PRESETS_C.map((preset) => preset != null)
const PRESET_SLOT_IDS = ['M1', 'M2', 'M3', 'M4', 'M5', 'M6', 'M7', 'M8', 'M9', 'M10']
const ADD_DEVICE_VALUE = '__add_device__'

const severityLabels: Record<DeviceSeverity, string> = {
  nominal: '就绪',
  warning: '检查',
  offline: '离线',
}

const transportLabels: Record<TransportKind, string> = {
  http: 'HTTP',
  serial: '串口',
  devd: 'DEVD',
  mock: '模拟',
  wifi: 'WiFi',
  bridge: '桥接',
}

const leaseStateLabels: Record<NonNullable<DeviceTarget['leaseState']>, string> = {
  none: '无',
  active: '有效',
  conflict: '冲突',
  expired: '过期',
}

const eventSourceLabels: Record<string, string> = {
  mock: '模拟',
  'usb-cdc': 'USB-CDC',
  pd: 'PD',
  flash: '烧录',
  probe: '探测',
  monitor: '监视',
  thermal: '热控',
  heater: '加热',
  devd: '本机桥接',
  ui: '界面',
  lease: '租约',
  serial: '串口',
}

const addDeviceOptions: Array<{
  kind: AddDeviceKind
  label: string
  detail: string
}> = [
  {
    kind: 'wifi',
    label: 'WiFi',
    detail: '预留后续站点地址，但不把硬件标记为在线。',
  },
  {
    kind: 'web-serial',
    label: 'Web Serial',
    detail: '打开浏览器 USB 串口并探测设备身份、网络与状态。',
  },
  {
    kind: 'bridge',
    label: '桥接',
    detail: '准备本机 devd 桥接目标，用于本地硬件控制。',
  },
]

const NO_LIVE_TARGET_ID = 'live-no-target'

const consoleViews: Array<{
  id: Exclude<ConsoleView, 'add-device'>
  label: string
  caption: string
  icon: typeof Gauge
}> = [
  {
    id: 'dashboard',
    label: '总览',
    caption: '温控运行',
    icon: Gauge,
  },
  {
    id: 'settings',
    label: '设置',
    caption: '温控策略',
    icon: SlidersHorizontal,
  },
  {
    id: 'calibration',
    label: '校准',
    caption: '标定工作台',
    icon: Wrench,
  },
  {
    id: 'update',
    label: '更新',
    caption: '固件检查',
    icon: Upload,
  },
]

function ConsoleViewLink({
  deviceId,
  view,
  calibrationTab,
  active,
  className,
  search,
  children,
}: {
  deviceId: string
  view: Exclude<ConsoleView, 'add-device'>
  calibrationTab: CalibrationWorkspaceTab
  active: boolean
  className: string
  search: AppSearch
  children: ReactNode
}) {
  const shared = {
    className,
    'aria-current': active ? ('page' as const) : undefined,
    search,
    children,
  }
  if (view === 'dashboard') {
    return <Link {...shared} to="/devices/$deviceId/overview" params={{ deviceId }} />
  }
  if (view === 'settings') {
    return <Link {...shared} to="/devices/$deviceId/settings" params={{ deviceId }} />
  }
  if (view === 'update') {
    return <Link {...shared} to="/devices/$deviceId/update" params={{ deviceId }} />
  }
  const calibrationPaths = {
    heater_curve: '/devices/$deviceId/calibration/heater-curve',
    rtd_adc: '/devices/$deviceId/calibration/rtd-adc',
    vin_adc: '/devices/$deviceId/calibration/vin-adc',
  } as const
  return <Link {...shared} to={calibrationPaths[calibrationTab]} params={{ deviceId }} />
}

function CalibrationRouteTab({
  navigation,
  tab,
  children,
}: {
  navigation?: ConsoleNavigationAdapter
  tab: CalibrationWorkspaceTab
  children: ReactNode
}) {
  if (!navigation || navigation.state.kind !== 'device') {
    return (
      <TabsTrigger value={tab} className="industrial-calibration-tab">
        <span>{children}</span>
      </TabsTrigger>
    )
  }
  const paths = {
    heater_curve: '/devices/$deviceId/calibration/heater-curve',
    rtd_adc: '/devices/$deviceId/calibration/rtd-adc',
    vin_adc: '/devices/$deviceId/calibration/vin-adc',
  } as const
  return (
    <TabsTrigger value={tab} className="industrial-calibration-tab" asChild>
      <Link
        to={paths[tab]}
        params={{ deviceId: navigation.state.deviceId }}
        search={navigation.search}
        aria-current={navigation.state.calibrationTab === tab ? 'page' : undefined}
        onPointerDownCapture={(event) => {
          if (
            event.button !== 0 ||
            event.altKey ||
            event.ctrlKey ||
            event.metaKey ||
            event.shiftKey
          ) {
            event.stopPropagation()
          }
        }}
        onClick={(event) => {
          if (
            event.button === 0 &&
            !event.altKey &&
            !event.ctrlKey &&
            !event.metaKey &&
            !event.shiftKey
          ) {
            event.preventDefault()
          }
        }}
      >
        <span>{children}</span>
      </Link>
    </TabsTrigger>
  )
}

function pendingDeviceId(kind: AddDeviceKind) {
  return `pending-${kind}-target`
}

function createPendingDevice(kind: AddDeviceKind): DeviceTarget {
  const common = {
    id: pendingDeviceId(kind),
    severity: 'offline' as const,
    firmware: 'pending',
    buildId: 'pending',
    uptime: 'pending',
    boardTempC: UNAVAILABLE_TEMPERATURE_C,
    currentTempC: UNAVAILABLE_TEMPERATURE_C,
    targetTempC: TARGET_TEMP_MIN,
    voltageMv: 0,
    currentMa: 0,
    pdRequestMv: 0,
    pdContractMv: 0,
    pdState: 'fault' as const,
    heaterEnabled: false,
    heaterOutputPercent: 0,
    manualPpsEnabled: false,
    manualPpsMv: null,
    manualPpsMa: null,
    ppsCapabilityMinMv: null,
    ppsCapabilityMaxMv: null,
    ppsCapabilityMaxMa: null,
    manualPpsError: null,
    heaterLockReason: null,
    calibration: {
      mode: 'off',
      ppsEnabled: false,
      ppsMv: null,
      ppsMa: null,
      heaterEnabled: false,
      targetAdcMv: null,
      stable: false,
      stabilityErrorMv: null,
      error: null,
      job: {
        kind: null,
        status: 'idle',
        progressPercent: 0,
        samplesCollected: 0,
        nextRequestMv: null,
        message: null,
      },
    },
    activeCoolingEnabled: false,
    fanState: 'OFF' as const,
    wifiRssi: null,
    capabilities: [],
    leaseState: 'none' as const,
  } satisfies Omit<DeviceTarget, 'alias' | 'location' | 'transport' | 'baseUrl'>

  if (kind === 'wifi') {
    return {
      ...common,
      alias: 'WiFi target',
      location: 'Awaiting WiFi handoff',
      transport: 'wifi',
      baseUrl: 'wifi://pending',
      networkState: 'idle',
      transportIssue: 'WiFi handoff is pending; no live station address is bound yet.',
    }
  }

  if (kind === 'bridge') {
    return {
      ...common,
      alias: 'Native bridge',
      location: 'Awaiting devd bridge',
      transport: 'bridge',
      baseUrl: 'bridge://pending',
      networkState: 'disabled',
      transportIssue: 'Start or select a native bridge target before runtime control.',
    }
  }

  return {
    ...common,
    alias: 'Web Serial target',
    location: 'Awaiting browser port',
    transport: 'serial',
    baseUrl: 'webserial://pending',
    networkState: 'disabled',
    transportIssue: 'Open this in live mode to select a browser Web Serial port.',
  }
}

function isRenderableTemperature(value: number) {
  return Number.isFinite(value) && value >= 0
}

export function shouldUseWifiReceipt(
  deviceSnapshot: Pick<DeviceTarget, 'configurationGeneration' | 'transitionSequence'>,
  receipt: Pick<NetworkSummary, 'configurationGeneration' | 'transitionSequence'>
) {
  const deviceGeneration = deviceSnapshot.configurationGeneration ?? 0
  const deviceSequence = deviceSnapshot.transitionSequence ?? 0
  const receiptGeneration = receipt.configurationGeneration ?? 0
  const receiptSequence = receipt.transitionSequence ?? 0

  return (
    receiptGeneration > deviceGeneration ||
    (receiptGeneration === deviceGeneration && receiptSequence > deviceSequence) ||
    // Firmware counters restart from a new transaction after a device reboot.
    // A direct configuration receipt with both counters lower is therefore
    // newer than the pre-reboot cached snapshot, not an out-of-order packet.
    (receiptGeneration < deviceGeneration && receiptSequence < deviceSequence)
  )
}

export function ControlPlaneDemo({
  scenario = controlPlaneScenario,
  initialView = 'dashboard',
  navigation,
  devd,
  webSerial: webSerialOptions,
  allowDemoControls = true,
  lanPairing,
  lanRuntime: lanRuntimeOptions,
}: ControlPlaneDemoProps) {
  const [selectedDeviceId, setSelectedDeviceId] = useState(scenario.selectedDeviceId)
  const [localActiveView, setLocalActiveView] = useState<ConsoleView>(initialView)
  const activeView = navigation
    ? navigation.state.kind === 'add-device'
      ? 'add-device'
      : navigation.state.view
    : localActiveView
  const [routePreferences, setRoutePreferences] = useState(readRoutePreferences)
  const [routeResumeFailed, setRouteResumeFailed] = useState(false)
  const [routeFallbackKind, setRouteFallbackKind] = useState<DeviceConnectionKind | undefined>()
  const automaticResumeKeyRef = useRef<string | null>(null)
  const successfulRouteKeyRef = useRef<string | null>(null)
  const previousRouteDeviceIdRef = useRef<string | null>(null)
  const [requestedConnectionByIdentity, setRequestedConnectionByIdentity] = useState<
    Record<string, { kind: DeviceConnectionKind; targetId: string }>
  >({})
  const invalidLanCredentialIdentityIdsRef = useRef(new Set<string>())
  const [selectedAddDeviceKind, setSelectedAddDeviceKind] =
    useState<AddDeviceKind>(defaultAddDeviceKind)
  const liveDevdScenario = useLiveDevdScenario(scenario, {
    ...devd,
    leaseEnabled: shouldHoldDevdLease(
      selectedDeviceId,
      activeView === 'add-device' && selectedAddDeviceKind === 'wifi'
    ),
  })
  const { scenario: liveScenario, serial: webSerial } = useLiveWebSerialScenario(liveDevdScenario, {
    ...webSerialOptions,
    persistKnownDevices: !allowDemoControls && webSerialOptions?.persistKnownDevices !== false,
  })
  const controlClient = useMemo(
    () => devd?.httpClient ?? createControlPlaneHttpClient(),
    [devd?.httpClient]
  )
  const devdBaseUrl = devd?.devdBaseUrl ?? defaultDevdBaseUrl()
  const lanRuntime = useMemo(
    () => ({
      createLease: lanRuntimeOptions?.createLease ?? createLanLease,
      releaseLease: lanRuntimeOptions?.releaseLease ?? releaseLanLease,
      startLeaseHeartbeat: lanRuntimeOptions?.startLeaseHeartbeat ?? startLanLeaseHeartbeat,
      streamEvents: lanRuntimeOptions?.streamEvents ?? streamLanEvents,
      probeDevice: lanRuntimeOptions?.probeDevice ?? probeLanDevice,
      getPublicInfo: lanRuntimeOptions?.getPublicInfo ?? getLanPublicInfo,
      writeRuntime: lanRuntimeOptions?.writeRuntime ?? writeLanRuntime,
      readStatus:
        lanRuntimeOptions?.readStatus ??
        ((session: LanDeviceSession) =>
          authorizedLanRequest<ControlPlaneStatus>(session, directLanStaleReadPath('runtime'))),
      readCalibration:
        lanRuntimeOptions?.readCalibration ??
        ((session: LanDeviceSession) =>
          authorizedLanRequest<CalibrationState>(session, directLanStaleReadPath('calibration'))),
      readHeaterCurve:
        lanRuntimeOptions?.readHeaterCurve ??
        ((session: LanDeviceSession) =>
          authorizedLanRequest<HeaterCurveState>(session, directLanStaleReadPath('heater-curve'))),
    }),
    [
      lanRuntimeOptions?.createLease,
      lanRuntimeOptions?.releaseLease,
      lanRuntimeOptions?.startLeaseHeartbeat,
      lanRuntimeOptions?.streamEvents,
      lanRuntimeOptions?.probeDevice,
      lanRuntimeOptions?.getPublicInfo,
      lanRuntimeOptions?.writeRuntime,
      lanRuntimeOptions?.readStatus,
      lanRuntimeOptions?.readCalibration,
      lanRuntimeOptions?.readHeaterCurve,
    ]
  )
  const [streamTick, setStreamTick] = useState(0)
  const [targetTempByDevice, setTargetTempByDevice] = useState<Record<string, number>>({})
  const [selectedPresetByDevice, setSelectedPresetByDevice] = useState<Record<string, number>>({})
  const [presetTempsByDevice, setPresetTempsByDevice] = useState<Record<string, number[]>>({})
  const [presetEnabledByDevice, setPresetEnabledByDevice] = useState<Record<string, boolean[]>>({})
  const [fanPolicyByDevice, setFanPolicyByDevice] = useState<
    Record<string, DeviceTarget['fanState']>
  >({})
  const [currentTempByDevice, setCurrentTempByDevice] = useState<Record<string, number>>({})
  const [heaterHeldByDevice, setHeaterHeldByDevice] = useState<Record<string, boolean>>({})
  const [manualPpsByDevice, setManualPpsByDevice] = useState<
    Record<string, { enabled: boolean; mv: number | null }>
  >({})
  const [calibrationRuntimeByDevice, setCalibrationRuntimeByDevice] = useState<
    Record<string, CalibrationRuntimeState>
  >({})
  const [calibrationByDevice, setCalibrationByDevice] = useState<Record<string, CalibrationState>>(
    {}
  )
  const calibrationVersionByDeviceRef = useRef<Record<string, number>>({})
  const [heaterCurveByDevice, setHeaterCurveByDevice] = useState<Record<string, HeaterCurveState>>(
    {}
  )
  const [calibrationWorkspaceTabByDevice, setCalibrationWorkspaceTabByDevice] = useState<
    Record<string, CalibrationWorkspaceTab>
  >({})
  const [calibrationRefsByDevice, setCalibrationRefsByDevice] = useState<
    Record<string, { rtdTempC: number; vinMv: number }>
  >({})
  const [artifactByDevice, setArtifactByDevice] = useState<Record<string, string>>({})
  const persistKnownWebSerialDevices =
    !allowDemoControls && webSerialOptions?.persistKnownDevices !== false
  const [pendingDevices, setPendingDevices] = useState<DeviceTarget[]>(() =>
    persistKnownWebSerialDevices
      ? listKnownWebSerialDevices().map(knownWebSerialDeviceToTarget)
      : []
  )
  const [wifiSnapshotsByDevice, setWifiSnapshotsByDevice] = useState<
    Record<string, NetworkSummary>
  >({})
  const [lanLeasesByDevice, setLanLeasesByDevice] = useState<Record<string, LanLease>>({})
  const pendingDeviceModeRef = useRef(allowDemoControls)
  const [flashRun, setFlashRun] = useState<{
    status: FlashRunStatus
    progress: number
  }>({
    status: 'idle',
    progress: 0,
  })
  const flashCompletionEmittedRef = useRef(false)
  const actionClockRef = useRef(LOG_FEED_START_SECONDS + 60)
  const targetTempCommitTimersRef = useRef<Record<string, number>>({})
  const targetTempCommitVersionRef = useRef<Record<string, number>>({})
  const [actionEvents, setActionEvents] = useState<EventLogEntry[]>([])
  const [pendingHeaterConfirmation, setPendingHeaterConfirmation] =
    useState<PendingHeaterConfirmation | null>(null)
  const [heaterConfirmationNow, setHeaterConfirmationNow] = useState(0)
  const [calibrationLeaveGuard, setCalibrationLeaveGuard] =
    useState<CalibrationLeaveGuardState | null>(null)
  const [feedback, setFeedback] = useState<ActionFeedback>({
    title: allowDemoControls ? '运行时已同步' : '暂无在线目标',
    detail: allowDemoControls
      ? '当前热控状态来自模拟设备契约。'
      : '连接浏览器 Web Serial 端口后即可加载真实硬件状态。',
    tone: 'info',
  })
  const activeScenario = liveScenario
  const deviceOptions = useMemo(
    () => [...activeScenario.devices, ...pendingDevices],
    [activeScenario.devices, pendingDevices]
  )
  const routeDeviceChoices = useMemo(
    () => mergeDeviceChoices(deviceOptions, { allowDemoControls }),
    [allowDemoControls, deviceOptions]
  )
  const routedConsoleState = navigation?.state
  const routeDeviceChoice =
    routedConsoleState?.kind === 'device'
      ? routeDeviceChoices.find((choice) => choice.identityId === routedConsoleState.deviceId)
      : undefined
  const preferredRouteDeviceConnection = routeDeviceChoice
    ? preferredDeviceConnection(
        routeDeviceChoice,
        requestedConnectionByIdentity[routeDeviceChoice.identityId]?.kind ??
          routeFallbackKind ??
          routePreferences.transportByIdentity[routeDeviceChoice.identityId],
        requestedConnectionByIdentity[routeDeviceChoice.identityId]?.targetId
      )
    : undefined
  const healthyRouteFallback = routeDeviceChoice?.connections
    .filter((connection) => connection !== preferredRouteDeviceConnection)
    .filter(isHealthyRouteConnection)
    .sort((left, right) => resumeConnectionPriority(left) - resumeConnectionPriority(right))[0]
  const routeDeviceConnection =
    preferredRouteDeviceConnection &&
    preferredRouteDeviceConnection.kind !== 'web-serial' &&
    !isHealthyRouteConnection(preferredRouteDeviceConnection) &&
    healthyRouteFallback
      ? healthyRouteFallback
      : preferredRouteDeviceConnection
  const preferredSelectedDeviceId = useMemo(() => {
    if (navigation?.state.kind === 'device' && routeDeviceConnection) {
      return routeDeviceConnection.target.id
    }
    const selectedOption = deviceOptions.find((device) => device.id === selectedDeviceId)
    if (!selectedOption) {
      return activeScenario.selectedDeviceId
    }

    if (selectedOption.connectionCandidate && selectedOption.connectionAvailable === false) {
      return activeScenario.selectedDeviceId
    }

    if (
      selectedDeviceId === scenario.selectedDeviceId &&
      activeScenario.selectedDeviceId !== scenario.selectedDeviceId
    ) {
      return activeScenario.selectedDeviceId
    }

    return selectedDeviceId
  }, [
    activeScenario.selectedDeviceId,
    deviceOptions,
    navigation?.state.kind,
    routeDeviceConnection,
    scenario.selectedDeviceId,
    selectedDeviceId,
  ])

  useEffect(() => {
    if (pendingDeviceModeRef.current === allowDemoControls) {
      return
    }

    pendingDeviceModeRef.current = allowDemoControls
    setPendingDevices([])
  }, [allowDemoControls])

  useEffect(() => {
    if (!persistKnownWebSerialDevices) return
    setPendingDevices((current) => {
      const remembered = listKnownWebSerialDevices().map(knownWebSerialDeviceToTarget)
      const rememberedIds = new Set(remembered.map((device) => device.id))
      return [...remembered, ...current.filter((device) => !rememberedIds.has(device.id))]
    })
  }, [persistKnownWebSerialDevices])

  useEffect(() => {
    if (allowDemoControls) {
      return
    }
    let cancelled = false
    for (const session of listSavedLanDeviceSessions()) {
      const rememberedTarget = savedLanSessionToDeviceTarget(session)
      const rememberedIdentityId = rememberedTarget ? deviceIdentityId(rememberedTarget) : null
      if (rememberedTarget) {
        setPendingDevices((current) =>
          upsertLanDeviceTarget(current, { ...rememberedTarget, connectionAvailable: false })
        )
      }
      if (!rememberedTarget || !rememberedIdentityId) continue
      void lanRuntime
        .getPublicInfo(session.baseUrl)
        .then((health) => resumeLanDeviceSession(session.baseUrl, health, lanRuntime.probeDevice))
        .then((resumed) => {
          if (cancelled) return
          if (!resumed) {
            setPendingDevices((current) =>
              current.filter((device) => device.id !== rememberedTarget.id)
            )
            return
          }
          const target = lanProbeToDeviceTarget(resumed.session, resumed.probe)
          setPendingDevices((current) => upsertLanDeviceTarget(current, target))
        })
        .catch((error: unknown) => {
          if (
            cancelled ||
            !(error instanceof ControlPlaneClientError) ||
            error.code !== 'unauthorized'
          ) {
            return
          }
          if (rememberedIdentityId) {
            invalidLanCredentialIdentityIdsRef.current.add(rememberedIdentityId)
          }
          setFeedback({
            title: 'LAN 配对凭据已失效',
            detail: '此设备的本地配对凭据已被撤销，请在 WiFi Info 页面重新进行物理配对。',
            tone: 'warning',
          })
        })
    }
    return () => {
      cancelled = true
    }
  }, [allowDemoControls, lanRuntime])

  useEffect(() => {
    if (!allowDemoControls || activeScenario.events.length < 2) {
      return
    }

    const timer = window.setInterval(() => {
      setStreamTick((tick) => tick + 1)
    }, 2200)

    return () => window.clearInterval(timer)
  }, [activeScenario.events.length, allowDemoControls])

  const selectedDevice = useMemo(
    () =>
      deviceOptions.find((device) => device.id === preferredSelectedDeviceId) ??
      deviceOptions.find((device) => device.id === activeScenario.selectedDeviceId) ??
      deviceOptions[0] ??
      activeScenario.devices[0],
    [
      activeScenario.devices,
      activeScenario.selectedDeviceId,
      deviceOptions,
      preferredSelectedDeviceId,
    ]
  )

  const routeDeviceId = navigation?.state.kind === 'device' ? navigation.state.deviceId : null

  useEffect(() => {
    if (previousRouteDeviceIdRef.current === routeDeviceId) return
    previousRouteDeviceIdRef.current = routeDeviceId
    setRouteResumeFailed(false)
    setRouteFallbackKind(undefined)
    automaticResumeKeyRef.current = null
    successfulRouteKeyRef.current = null
  }, [routeDeviceId])

  useEffect(() => {
    const routeTarget = routeDeviceConnection?.target
    if (!navigation || navigation.state.kind !== 'device' || !routeTarget) return
    if (selectedDeviceId !== routeTarget.id) setSelectedDeviceId(routeTarget.id)
  }, [navigation, routeDeviceConnection, selectedDeviceId])

  useEffect(() => {
    if (!deviceOptions.some((device) => device.id === selectedDeviceId)) {
      setSelectedDeviceId(activeScenario.selectedDeviceId)
      return
    }

    if (
      selectedDeviceId === scenario.selectedDeviceId &&
      activeScenario.selectedDeviceId !== scenario.selectedDeviceId
    ) {
      setSelectedDeviceId(activeScenario.selectedDeviceId)
    }
  }, [activeScenario.selectedDeviceId, deviceOptions, scenario.selectedDeviceId, selectedDeviceId])

  useEffect(() => {
    if (webSerial.state !== 'connected' || !webSerial.deviceId) {
      return
    }

    const connectedTarget = activeScenario.devices.find(
      (device) => device.id === webSerial.deviceId
    )
    if (navigation?.state.kind === 'add-device' && connectedTarget) {
      void navigation.navigate({
        kind: 'device',
        deviceId: deviceIdentityId(connectedTarget),
        view: 'dashboard',
      })
    }
    if (persistKnownWebSerialDevices && connectedTarget) {
      const deviceId = connectedTarget.identityId ?? connectedTarget.id.replace(/^web-serial-/, '')
      const remembered = {
        deviceId,
        hostname: connectedTarget.alias,
        firmwareVersion: connectedTarget.firmware,
        buildId: connectedTarget.buildId,
      }
      rememberKnownWebSerialDevice(remembered)
      const hint = knownWebSerialDeviceToTarget(remembered)
      setPendingDevices((current) => {
        const existing = current.find((device) => device.id === hint.id)
        if (
          existing?.alias === hint.alias &&
          existing.firmware === hint.firmware &&
          existing.buildId === hint.buildId
        ) {
          return current
        }
        return [hint, ...current.filter((device) => device.id !== hint.id)]
      })
    }

    const currentSelection = deviceOptions.find((device) => device.id === preferredSelectedDeviceId)
    const shouldAdoptWebSerialTarget =
      selectedAddDeviceKind === 'web-serial' ||
      !currentSelection ||
      isNoLiveTargetDevice(currentSelection) ||
      isPendingDeviceChoice(currentSelection)

    if (shouldAdoptWebSerialTarget && preferredSelectedDeviceId !== webSerial.deviceId) {
      setSelectedDeviceId(webSerial.deviceId)
    }
  }, [
    deviceOptions,
    preferredSelectedDeviceId,
    activeScenario.devices,
    navigation,
    persistKnownWebSerialDevices,
    selectedAddDeviceKind,
    webSerial.deviceId,
    webSerial.state,
  ])

  useEffect(() => {
    if (selectedAddDeviceKind !== 'web-serial' || webSerial.state !== 'error') {
      return
    }

    setFeedback({
      title: 'Web Serial unavailable',
      detail: webSerial.error ?? 'Browser direct USB control could not be opened.',
      tone: 'warning',
    })
  }, [selectedAddDeviceKind, webSerial.error, webSerial.state])

  useEffect(() => {
    if (webSerial.state !== 'connected') {
      return
    }

    setFeedback((current) => clearStaleWebSerialFailure(current))
  }, [webSerial.state])

  useEffect(() => {
    const nextSelectedDevice = activeScenario.devices.find(
      (device) => device.id === activeScenario.selectedDeviceId
    )
    if (
      nextSelectedDevice?.transport === 'devd' &&
      nextSelectedDevice.connectionAvailable !== false &&
      (feedback.detail === '当前热控状态来自模拟设备契约。' ||
        feedback.detail === '连接浏览器 Web Serial 端口后即可加载真实硬件状态。')
    ) {
      setFeedback({
        title: '运行时已同步',
        detail: '当前热控状态来自 devd 固件状态。',
        tone: 'info',
      })
    }
  }, [activeScenario.devices, activeScenario.selectedDeviceId, feedback.detail])

  useEffect(() => {
    if (webSerial.state === 'error' && webSerial.error) {
      setFeedback({
        title: 'Web Serial unavailable',
        detail: webSerial.error,
        tone: 'warning',
      })
    }
  }, [webSerial.error, webSerial.state])

  useEffect(() => {
    const liveDevdDevice = activeScenario.devices.find((device) => device.transport === 'devd')
    if (!liveDevdDevice || LIVE_DEVD_TRANSIENT_DEVICE_IDS.has(liveDevdDevice.id)) {
      return
    }

    const nextDeviceId = liveDevdDevice.id
    const previousIds = LIVE_DEVD_TRANSIENT_DEVICE_IDS

    const migrateRecord = <T,>(
      setter: Dispatch<SetStateAction<Record<string, T>>>,
      clone?: (value: T) => T
    ) => {
      setter((current) => {
        if (current[nextDeviceId] !== undefined) {
          return current
        }

        const sourceId = Array.from(previousIds).find((deviceId) => current[deviceId] !== undefined)
        if (!sourceId) {
          return current
        }

        const value = current[sourceId]
        if (value === undefined) {
          return current
        }

        const next = {
          ...current,
          [nextDeviceId]: clone ? clone(value) : value,
        }
        delete next[sourceId]
        return next
      })
    }

    migrateRecord(setTargetTempByDevice)
    migrateRecord(setSelectedPresetByDevice)
    migrateRecord(setPresetTempsByDevice, (value) => [...value])
    migrateRecord(setPresetEnabledByDevice, (value) => [...value])
    migrateRecord(setFanPolicyByDevice)
    migrateRecord(setHeaterHeldByDevice)
    migrateRecord(setManualPpsByDevice, (value) => ({ ...value }))
    migrateRecord(setCalibrationRuntimeByDevice, (value) => ({
      ...value,
      job: { ...value.job },
    }))
    migrateRecord(setCalibrationByDevice, cloneCalibrationState)
    migrateRecord(setHeaterCurveByDevice, (value) => ({
      active: cloneHeaterCurvePackage(value.active),
      preview: value.preview ? cloneHeaterCurvePackage(value.preview) : null,
    }))
    migrateRecord(setCalibrationWorkspaceTabByDevice)
    migrateRecord(setCalibrationRefsByDevice, (value) => ({ ...value }))
    migrateRecord(setArtifactByDevice)

    if (selectedDeviceId && LIVE_DEVD_TRANSIENT_DEVICE_IDS.has(selectedDeviceId)) {
      setSelectedDeviceId(nextDeviceId)
    }
    if (
      pendingHeaterConfirmation &&
      LIVE_DEVD_TRANSIENT_DEVICE_IDS.has(pendingHeaterConfirmation.deviceId)
    ) {
      setPendingHeaterConfirmation((current) =>
        current && LIVE_DEVD_TRANSIENT_DEVICE_IDS.has(current.deviceId)
          ? { ...current, deviceId: nextDeviceId }
          : current
      )
    }
  }, [activeScenario.devices, pendingHeaterConfirmation, selectedDeviceId])

  useEffect(() => {
    setTargetTempByDevice((current) => {
      let next = current
      for (const device of activeScenario.devices) {
        if (
          !(device.transport === 'devd' || isDirectWebSerialDevice(device)) ||
          current[device.id] !== device.targetTempC
        ) {
          continue
        }
        if (next === current) {
          next = { ...current }
        }
        delete next[device.id]
      }
      return next
    })
  }, [activeScenario.devices])

  useEffect(() => {
    setCalibrationRuntimeByDevice((current) => {
      let next = current
      for (const device of activeScenario.devices) {
        const localRuntime = current[device.id]
        if (!localRuntime) {
          continue
        }
        if (
          localRuntime.mode !== device.calibration.mode ||
          localRuntime.ppsEnabled !== device.calibration.ppsEnabled ||
          localRuntime.ppsMv !== device.calibration.ppsMv ||
          localRuntime.ppsMa !== device.calibration.ppsMa ||
          localRuntime.heaterEnabled !== device.calibration.heaterEnabled ||
          localRuntime.targetAdcMv !== device.calibration.targetAdcMv ||
          localRuntime.stable !== device.calibration.stable ||
          localRuntime.stabilityErrorMv !== device.calibration.stabilityErrorMv ||
          localRuntime.error !== device.calibration.error ||
          localRuntime.job.kind !== device.calibration.job.kind ||
          localRuntime.job.status !== device.calibration.job.status ||
          localRuntime.job.progressPercent !== device.calibration.job.progressPercent ||
          localRuntime.job.samplesCollected !== device.calibration.job.samplesCollected ||
          localRuntime.job.nextRequestMv !== device.calibration.job.nextRequestMv ||
          localRuntime.job.message !== device.calibration.job.message
        ) {
          continue
        }
        if (next === current) {
          next = { ...current }
        }
        delete next[device.id]
      }
      return next
    })
  }, [activeScenario.devices])

  useEffect(() => {
    setSelectedPresetByDevice((current) => {
      let next = current
      for (const device of activeScenario.devices) {
        if (
          !(device.transport === 'devd' || isDirectWebSerialDevice(device)) ||
          current[device.id] !== clampPresetIndex(device.selectedPresetIndex)
        ) {
          continue
        }
        if (next === current) {
          next = { ...current }
        }
        delete next[device.id]
      }
      return next
    })
  }, [activeScenario.devices])

  useEffect(() => {
    return () => {
      for (const timer of Object.values(targetTempCommitTimersRef.current)) {
        window.clearTimeout(timer)
      }
    }
  }, [])

  useEffect(() => {
    setFanPolicyByDevice((current) => {
      let next = current
      for (const device of activeScenario.devices) {
        if (
          !(device.transport === 'devd' || isDirectWebSerialDevice(device)) ||
          current[device.id] !== device.fanState
        ) {
          continue
        }
        if (next === current) {
          next = { ...current }
        }
        delete next[device.id]
      }
      return next
    })
  }, [activeScenario.devices])

  const visibleDevice = useMemo(() => {
    if (!selectedDevice) {
      return activeScenario.devices[0]
    }

    const liveRuntimeDevice = isLiveRuntimeDevice(selectedDevice)
    const currentTempC = liveRuntimeDevice
      ? selectedDevice.currentTempC
      : (currentTempByDevice[selectedDevice.id] ?? selectedDevice.currentTempC)
    const targetTempC = targetTempByDevice[selectedDevice.id] ?? selectedDevice.targetTempC
    const fanState = liveRuntimeDevice
      ? selectedDevice.fanState
      : (fanPolicyByDevice[selectedDevice.id] ?? selectedDevice.fanState)
    const heaterOutputPercent =
      selectedDevice.severity === 'offline'
        ? selectedDevice.heaterOutputPercent
        : liveRuntimeDevice
          ? selectedDevice.heaterOutputPercent
          : !isRenderableTemperature(currentTempC)
            ? selectedDevice.heaterOutputPercent
            : Math.min(
                100,
                Math.max(
                  0,
                  selectedDevice.heaterOutputPercent + Math.round((targetTempC - currentTempC) / 8)
                )
              )
    const manualPpsOverride = liveRuntimeDevice ? undefined : manualPpsByDevice[selectedDevice.id]
    const manualPpsEnabled = manualPpsOverride?.enabled ?? selectedDevice.manualPpsEnabled ?? false
    const manualPpsMv = manualPpsOverride
      ? manualPpsOverride.mv
      : (selectedDevice.manualPpsMv ?? null)
    const lanLease = isDirectLanDevice(selectedDevice)
      ? lanLeasesByDevice[selectedDevice.id]
      : undefined
    const networkSnapshot = wifiSnapshotsByDevice[selectedDevice.id]
    const useNetworkSnapshot =
      networkSnapshot != null && shouldUseWifiReceipt(selectedDevice, networkSnapshot)
    const effectiveNetwork = useNetworkSnapshot ? networkSnapshot : undefined
    return {
      ...selectedDevice,
      currentTempC,
      targetTempC,
      fanState,
      activeCoolingEnabled: selectedDevice.activeCoolingEnabled,
      heaterEnabled: heaterHeldByDevice[selectedDevice.id] ? false : selectedDevice.heaterEnabled,
      heaterOutputPercent: heaterHeldByDevice[selectedDevice.id] ? 0 : heaterOutputPercent,
      manualPpsEnabled,
      manualPpsMv,
      pdRequestMv:
        manualPpsEnabled && manualPpsMv != null ? manualPpsMv : selectedDevice.pdRequestMv,
      pdContractMv:
        manualPpsEnabled && manualPpsMv != null ? manualPpsMv : selectedDevice.pdContractMv,
      voltageMv: manualPpsEnabled && manualPpsMv != null ? manualPpsMv : selectedDevice.voltageMv,
      wifiSsid: effectiveNetwork?.ssid ?? selectedDevice.wifiSsid,
      wifiRssi: effectiveNetwork?.wifiRssi ?? selectedDevice.wifiRssi,
      wifiPasswordLength: effectiveNetwork?.wifiPasswordLength ?? selectedDevice.wifiPasswordLength,
      networkState: effectiveNetwork?.state ?? selectedDevice.networkState,
      configurationGeneration:
        effectiveNetwork?.configurationGeneration ?? selectedDevice.configurationGeneration,
      transitionSequence: effectiveNetwork?.transitionSequence ?? selectedDevice.transitionSequence,
      wifiFailureCode: effectiveNetwork?.failureCode ?? selectedDevice.wifiFailureCode,
      leaseId: lanLease?.leaseId ?? selectedDevice.leaseId,
      leaseState: isDirectLanDevice(selectedDevice)
        ? lanLease
          ? 'active'
          : selectedDevice.leaseState
        : selectedDevice.leaseState,
    }
  }, [
    activeScenario.devices,
    currentTempByDevice,
    fanPolicyByDevice,
    heaterHeldByDevice,
    manualPpsByDevice,
    lanLeasesByDevice,
    selectedDevice,
    targetTempByDevice,
    wifiSnapshotsByDevice,
  ])
  const visibleDeviceIsLive = isLiveRuntimeDevice(visibleDevice)
  const lanDeviceId = isDirectLanDevice(visibleDevice) ? visibleDevice.id : undefined
  const lanDeviceBaseUrl = isDirectLanDevice(visibleDevice) ? visibleDevice.baseUrl : undefined
  const lanLease = lanDeviceId ? lanLeasesByDevice[lanDeviceId] : undefined
  const lanLeaseRequest = useMemo(
    () =>
      lanLeaseAcquisitionRequest(
        {
          id: visibleDevice.id,
          alias: visibleDevice.alias,
          baseUrl: visibleDevice.baseUrl,
          transport: visibleDevice.transport,
          leaseState: visibleDevice.leaseState,
        },
        Boolean(lanLease)
      ),
    [
      lanLease,
      visibleDevice.alias,
      visibleDevice.baseUrl,
      visibleDevice.id,
      visibleDevice.leaseState,
      visibleDevice.transport,
    ]
  )

  useEffect(() => {
    if (!lanLeaseRequest) {
      return
    }
    const { alias, baseUrl, deviceId } = lanLeaseRequest
    const session = loadLanDeviceSession(baseUrl)
    if (!session) {
      setPendingDevices((current) =>
        current.map((device) =>
          device.id === deviceId
            ? {
                ...device,
                networkState: 'error',
                transportIssue: '本机未保存该设备的配对凭据，请重新配对。',
              }
            : device
        )
      )
      return
    }
    let retired = false
    void lanRuntime
      .createLease(session)
      .then((created) => {
        if (retired) {
          void lanRuntime.releaseLease(session, created.leaseId).catch(() => undefined)
          return
        }
        setLanLeasesByDevice((current) => ({
          ...current,
          [deviceId]: created,
        }))
        setFeedback((current) =>
          current.title === 'LAN 设备已配对' || current.title === '正在获取 LAN 租约'
            ? {
                title: 'LAN 设备已连接',
                detail: `${alias} 已取得控制 lease。`,
                tone: 'success',
              }
            : current
        )
      })
      .catch((error: unknown) => {
        if (retired) return
        setPendingDevices((current) =>
          current.map((device) =>
            device.id === deviceId
              ? {
                  ...device,
                  leaseState: 'conflict',
                  transportIssue: error instanceof Error ? error.message : '无法获取 LAN lease。',
                }
              : device
          )
        )
      })
    return () => {
      retired = true
    }
  }, [lanLeaseRequest, lanRuntime])

  useEffect(() => {
    if (!lanDeviceId || !lanDeviceBaseUrl || !lanLease) {
      return
    }
    const session = loadLanDeviceSession(lanDeviceBaseUrl)
    if (!session) {
      return
    }
    let retired = false
    const stopHeartbeat = lanRuntime.startLeaseHeartbeat(session, lanLease, (error) => {
      if (retired) return
      setLanLeasesByDevice((current) => {
        if (current[lanDeviceId]?.leaseId !== lanLease.leaseId) {
          return current
        }
        const next = { ...current }
        delete next[lanDeviceId]
        return next
      })
      setPendingDevices((current) =>
        current.map((device) =>
          device.id === lanDeviceId
            ? {
                ...device,
                leaseState: 'expired',
                transportIssue: lanLeaseHeartbeatFailureDetail(error.message),
              }
            : device
        )
      )
    })
    return () => {
      retired = true
      stopHeartbeat()
      void lanRuntime.releaseLease(session, lanLease.leaseId).catch(() => undefined)
      setLanLeasesByDevice((current) => {
        if (current[lanDeviceId]?.leaseId !== lanLease.leaseId) {
          return current
        }
        const next = { ...current }
        delete next[lanDeviceId]
        return next
      })
    }
  }, [lanDeviceBaseUrl, lanDeviceId, lanLease, lanRuntime])

  useEffect(() => {
    if (!lanDeviceId || !lanDeviceBaseUrl) {
      return
    }
    const session = loadLanDeviceSession(lanDeviceBaseUrl)
    if (!session) {
      return
    }
    const controller = new AbortController()
    void (async () => {
      try {
        for await (const event of lanRuntime.streamEvents(session, controller.signal)) {
          if (controller.signal.aborted || !isControlPlaneStatus(event)) {
            continue
          }
          setPendingDevices((current) =>
            current.map((device) =>
              device.id === lanDeviceId ? applyLanStatus(device, event) : device
            )
          )
          setTargetTempByDevice((current) => ({
            ...current,
            [lanDeviceId]: event.targetTempC,
          }))
          setManualPpsByDevice((current) => ({
            ...current,
            [lanDeviceId]: {
              enabled: event.manualPpsEnabled ?? false,
              mv: event.manualPpsMv ?? null,
            },
          }))
          setFanPolicyByDevice((current) => ({
            ...current,
            [lanDeviceId]: event.fanDisplayState,
          }))
          setCalibrationRuntimeByDevice((current) => ({
            ...current,
            [lanDeviceId]: event.calibration,
          }))
          setHeaterHeldByDevice((current) => ({
            ...current,
            [lanDeviceId]: !event.heaterEnabled,
          }))
        }
      } catch (error) {
        if (controller.signal.aborted) {
          return
        }
        setPendingDevices((current) =>
          current.map((device) =>
            device.id === lanDeviceId
              ? {
                  ...device,
                  networkState: 'error',
                  transportIssue: error instanceof Error ? error.message : 'LAN 事件流已断开。',
                }
              : device
          )
        )
      }
    })()
    return () => controller.abort()
  }, [lanDeviceBaseUrl, lanDeviceId, lanRuntime])
  const visiblePresetValues =
    visibleDeviceIsLive && visibleDevice.presetsC
      ? normalizePresets(visibleDevice.presetsC)
      : presetValuesFromEditorState(
          presetTempsByDevice[visibleDevice.id] ?? PRESET_TEMPS_C,
          presetEnabledByDevice[visibleDevice.id] ?? PRESET_ENABLED
        )
  const selectedPresetIndex = visibleDeviceIsLive
    ? (selectedPresetByDevice[visibleDevice.id] ??
      clampPresetIndex(visibleDevice.selectedPresetIndex))
    : (selectedPresetByDevice[visibleDevice.id] ?? 3)
  const visiblePresetTemps = presetTempsFromValues(visiblePresetValues)
  const visiblePresetEnabled = presetEnabledFromValues(visiblePresetValues)
  const visibleFanPolicy = fanPolicyByDevice[visibleDevice.id] ?? fanPolicyFromDevice(visibleDevice)
  const visibleCalibration =
    calibrationByDevice[visibleDevice.id] ??
    activeScenario.devices.find((device) => device.id === visibleDevice.id)?.storedCalibration ??
    createDefaultCalibrationState()
  const visibleRuntimeCalibration =
    calibrationRuntimeByDevice[visibleDevice.id] ?? visibleDevice.calibration
  const visibleHeaterCurve =
    heaterCurveByDevice[visibleDevice.id] ??
    activeScenario.devices.find((device) => device.id === visibleDevice.id)?.heaterCurve ??
    createDefaultHeaterCurveState()
  const visibleCalibrationWorkspaceTab =
    navigation?.state.kind === 'device' && navigation.state.view === 'calibration'
      ? (navigation.state.calibrationTab ?? 'heater_curve')
      : (calibrationWorkspaceTabByDevice[visibleDevice.id] ?? 'heater_curve')
  const visibleCalibrationRefs = calibrationRefsByDevice[visibleDevice.id] ?? {
    rtdTempC: isRenderableTemperature(visibleDevice.currentTempC)
      ? Number(visibleDevice.currentTempC.toFixed(1))
      : 25,
    vinMv: visibleDevice.voltageMv,
  }

  const setConsoleView = useCallback(
    (nextView: ConsoleView, options?: { replace?: boolean }) => {
      if (!navigation) {
        setLocalActiveView(nextView)
        return Promise.resolve()
      }
      if (nextView === 'add-device') {
        return navigation.navigate({ kind: 'add-device' }, options)
      }
      const deviceId =
        navigation.state.kind === 'device'
          ? navigation.state.deviceId
          : deviceIdentityId(visibleDevice)
      return navigation.navigate(
        {
          kind: 'device',
          deviceId,
          view: nextView,
          ...(nextView === 'calibration' ? { calibrationTab: visibleCalibrationWorkspaceTab } : {}),
        },
        options
      )
    },
    [navigation, visibleCalibrationWorkspaceTab, visibleDevice]
  )

  const setWorkspaceTab = useCallback(
    (
      nextTab: CalibrationWorkspaceTab,
      options?: { replace?: boolean; ignoreBlocker?: boolean }
    ) => {
      if (!navigation || navigation.state.kind !== 'device') {
        setCalibrationWorkspaceTabByDevice((current) => ({
          ...current,
          [visibleDevice.id]: nextTab,
        }))
        return Promise.resolve()
      }
      return navigation.navigate(
        {
          kind: 'device',
          deviceId: navigation.state.deviceId,
          view: 'calibration',
          calibrationTab: nextTab,
        },
        options
      )
    },
    [navigation, visibleDevice.id]
  )

  const onCalibrationGuardChange = navigation?.onCalibrationGuardChange
  const navigationDeviceId = navigation?.state.kind === 'device' ? navigation.state.deviceId : null

  useEffect(() => {
    if (!onCalibrationGuardChange) return
    const guard =
      activeView === 'calibration' && visibleRuntimeCalibration.mode !== 'off'
        ? {
            deviceId: navigationDeviceId ?? deviceIdentityId(visibleDevice),
            workspaceTab: visibleCalibrationWorkspaceTab,
          }
        : null
    onCalibrationGuardChange(guard)
    return () => onCalibrationGuardChange(null)
  }, [
    activeView,
    navigationDeviceId,
    onCalibrationGuardChange,
    visibleCalibrationWorkspaceTab,
    visibleDevice,
    visibleRuntimeCalibration.mode,
  ])

  useEffect(() => {
    const blocked = navigation?.blockedNavigation
    if (!blocked) return
    setCalibrationLeaveGuard((current) => {
      if (current?.continueAction === blocked.proceed && current.cancelAction === blocked.reset) {
        return current
      }
      return {
        reason: 'view-change',
        nextLabel: blocked.nextLabel,
        nextView: blocked.next?.kind === 'device' ? blocked.next.view : 'add-device',
        nextWorkspaceTab: blocked.next?.kind === 'device' ? blocked.next.calibrationTab : undefined,
        continueAction: blocked.proceed,
        cancelAction: blocked.reset,
      }
    })
    setFeedback({
      title: '请先关闭校准控制',
      detail: `${calibrationModeLabel(visibleCalibrationWorkspaceTab)}仍在运行，离开前请先关闭开关。`,
      tone: 'warning',
    })
  }, [navigation?.blockedNavigation, visibleCalibrationWorkspaceTab])

  const connectPreauthorizedWebSerial = useCallback(
    (signal: AbortSignal, expectedIdentityId: string) =>
      webSerial.connect({
        replaceExisting: true,
        preauthorizedOnly: true,
        signal,
        expectedIdentityId,
      }),
    [webSerial.connect]
  )

  const routeRecoveryIdentityId =
    navigation?.state.kind === 'device' ? navigation.state.deviceId : null
  const routeRecoveryVariant = navigation?.variant
  const routeConnectionKind = routeDeviceConnection?.kind
  const routeConnectionTargetId = routeDeviceConnection?.target.id
  const routeConnectionTargetAlias = routeDeviceConnection?.target.alias
  const routeConnectionUnavailable = routeDeviceConnection
    ? routeDeviceConnection.target.connectionAvailable === false ||
      routeDeviceConnection.target.severity === 'offline'
    : true
  const routeFallbackConnection = routeDeviceChoice?.connections
    .filter((connection) => connection.kind !== 'web-serial')
    .filter(isHealthyRouteConnection)
    .sort((left, right) => resumeConnectionPriority(left) - resumeConnectionPriority(right))[0]
  const routeFallbackConnectionKind = routeFallbackConnection?.kind
  const routeFallbackConnectionLabel = routeFallbackConnection?.label

  useEffect(() => {
    if (
      !routeRecoveryIdentityId ||
      !routeRecoveryVariant ||
      !routeConnectionKind ||
      !routeConnectionTargetId ||
      !routeConnectionTargetAlias
    ) {
      return
    }
    if (allowDemoControls) {
      setRouteResumeFailed(routeConnectionUnavailable)
      return
    }
    if (routeConnectionKind !== 'web-serial') {
      setRouteResumeFailed(routeConnectionUnavailable)
      return
    }
    if (webSerial.deviceIdentityId === routeRecoveryIdentityId) {
      setRouteResumeFailed(false)
      return
    }
    if (!webSerial.preauthorizedPortsReady) return

    const attemptKey = `${routeRecoveryVariant}:${routeRecoveryIdentityId}:${routeConnectionTargetId}`
    if (automaticResumeKeyRef.current === attemptKey) return
    automaticResumeKeyRef.current = attemptKey
    setFeedback({
      title: '正在恢复 Web Serial',
      detail: `正在使用已授权端口验证 ${routeConnectionTargetAlias}，不会打开系统设备选择器。`,
      tone: 'info',
    })
    const controller = new AbortController()
    void connectPreauthorizedWebSerial(controller.signal, routeRecoveryIdentityId).then(
      (connected) => {
        if (controller.signal.aborted) return
        if (connected) {
          setFeedback({
            title: 'Web Serial connected',
            detail: 'Browser direct USB JSONL control is active.',
            tone: 'success',
          })
          setRouteResumeFailed(false)
          return
        }
        if (routeFallbackConnectionKind && routeFallbackConnectionLabel) {
          setRequestedConnectionByIdentity((current) => {
            const next = { ...current }
            delete next[routeRecoveryIdentityId]
            return next
          })
          setRouteFallbackKind(routeFallbackConnectionKind)
          setFeedback({
            title: '已切换备用连接',
            detail: `${routeConnectionTargetAlias} 的预授权串口不可用，正在尝试 ${routeFallbackConnectionLabel}。`,
            tone: 'warning',
          })
          return
        }
        setFeedback({
          title: 'Web Serial unavailable',
          detail: 'Browser direct USB control could not be opened.',
          tone: 'warning',
        })
        setRouteResumeFailed(true)
      }
    )
    return () => controller.abort()
  }, [
    connectPreauthorizedWebSerial,
    allowDemoControls,
    routeConnectionKind,
    routeConnectionUnavailable,
    routeConnectionTargetAlias,
    routeConnectionTargetId,
    routeFallbackConnectionKind,
    routeFallbackConnectionLabel,
    routeRecoveryIdentityId,
    routeRecoveryVariant,
    webSerial.deviceIdentityId,
    webSerial.preauthorizedPortsReady,
  ])

  useEffect(() => {
    if (!navigation || navigation.state.kind !== 'device' || !routeDeviceConnection) return
    const identityId = navigation.state.deviceId
    if (deviceIdentityId(visibleDevice) !== identityId) return
    const successful =
      visibleDevice.connectionAvailable !== false &&
      visibleDevice.severity === 'nominal' &&
      (routeDeviceConnection.kind === 'mock' || visibleDevice.leaseState === 'active')
    if (!successful) return
    const successKey = `${navigation.variant}:${identityId}:${routeDeviceConnection.kind}`
    if (successfulRouteKeyRef.current === successKey) return
    successfulRouteKeyRef.current = successKey
    rememberSuccessfulRoute(navigation.variant, identityId, routeDeviceConnection.kind)
    setRequestedConnectionByIdentity((current) => {
      if (!(identityId in current)) return current
      const next = { ...current }
      delete next[identityId]
      return next
    })
    setRoutePreferences(readRoutePreferences())
  }, [navigation, routeDeviceConnection, visibleDevice])

  useEffect(() => {
    if (!calibrationLeaveGuard) {
      return
    }

    if (activeView !== 'calibration' || visibleRuntimeCalibration.mode === 'off') {
      setCalibrationLeaveGuard(null)
    }
  }, [activeView, calibrationLeaveGuard, visibleRuntimeCalibration.mode])

  useEffect(() => {
    if (activeView !== 'calibration') {
      return
    }

    const activeMode = asWorkbenchMode(visibleRuntimeCalibration.mode)
    if (!activeMode || activeMode === visibleCalibrationWorkspaceTab) {
      return
    }

    void setWorkspaceTab(activeMode, { replace: true, ignoreBlocker: true })
  }, [activeView, setWorkspaceTab, visibleCalibrationWorkspaceTab, visibleRuntimeCalibration.mode])

  useEffect(() => {
    if (!shouldShowDeviceControlBlockFeedback(visibleDevice)) {
      return
    }

    if (invalidLanCredentialIdentityIdsRef.current.has(deviceIdentityId(visibleDevice))) {
      return
    }

    if (
      activeView === 'add-device' &&
      selectedAddDeviceKind === 'web-serial' &&
      (webSerial.state === 'connecting' || webSerial.state === 'error')
    ) {
      return
    }

    const blockedReason = deviceControlBlockReason(visibleDevice)
    if (!blockedReason) {
      return
    }

    const conflictTitle =
      visibleDevice.leaseState === 'conflict'
        ? '设备租约冲突'
        : isDirectLanDevice(visibleDevice) && visibleDevice.leaseState === 'none'
          ? '正在获取 LAN 租约'
          : '硬件连接受阻'
    setFeedback((current) => {
      if (current.detail === blockedReason && current.title === conflictTitle) {
        return current
      }
      return {
        title: conflictTitle,
        detail: blockedReason,
        tone: 'warning',
      }
    })
  }, [activeView, selectedAddDeviceKind, visibleDevice, webSerial.state])

  useEffect(() => {
    if (!visibleDeviceIsLive || !visibleDevice.heaterLockReason) {
      return
    }

    const detail = heaterLockReasonText(visibleDevice.heaterLockReason)
    setFeedback((current) => {
      if (!shouldReplacePassiveFeedbackWithHeaterLock(current.title)) {
        return current
      }
      if (current.title === '加热安全锁已触发' && current.detail === detail) {
        return current
      }
      return {
        title: '加热安全锁已触发',
        detail,
        tone: 'warning',
      }
    })
  }, [visibleDevice, visibleDeviceIsLive])

  useEffect(() => {
    if (!pendingHeaterConfirmation) {
      return
    }

    const remainingMs = Math.max(
      0,
      HEATER_CONFIRMATION_TIMEOUT_MS - (Date.now() - pendingHeaterConfirmation.requestedAtMs)
    )
    const timer = window.setTimeout(() => {
      setHeaterConfirmationNow(Date.now())
    }, remainingMs)

    return () => window.clearTimeout(timer)
  }, [pendingHeaterConfirmation])

  useEffect(() => {
    if (!visibleDeviceIsLive) {
      return
    }

    const blockedReason = deviceControlBlockReason(visibleDevice)
    if (blockedReason) {
      return
    }

    setFeedback((current) => {
      if (!isTransportBlockedFeedback(current)) {
        return current
      }
      return {
        title: '运行时已同步',
        detail: isDirectWebSerialDevice(visibleDevice)
          ? '当前热控状态来自浏览器 Web Serial。'
          : '当前热控状态来自 devd 固件状态。',
        tone: 'info',
      }
    })
  }, [visibleDevice, visibleDeviceIsLive])
  const selectedArtifact = useMemo(
    () =>
      activeScenario.artifacts.find(
        (artifact) => artifact.id === artifactByDevice[visibleDevice.id]
      ) ?? activeScenario.artifacts[0],
    [activeScenario.artifacts, artifactByDevice, visibleDevice.id]
  )
  const visibleFlashPhases = useMemo(
    () => createFlashPhases(activeScenario.flashPhases, selectedArtifact, visibleDevice, flashRun),
    [activeScenario.flashPhases, flashRun, selectedArtifact, visibleDevice]
  )
  const knownDevices = useMemo(
    () => deviceOptions.filter((device) => isKnownDeviceChoice(device)),
    [deviceOptions]
  )
  const isDeviceSelectionRequired = isNoLiveTargetDevice(visibleDevice)
  const showDeviceSelection = isDeviceSelectionRequired && activeView !== 'add-device'
  const isDeviceAddFlowActive = isDeviceSelectionRequired || activeView === 'add-device'
  const visibleDeviceId = visibleDevice.id
  const visibleDeviceTransport = visibleDevice.transport
  const visibleDeviceLeaseId = visibleDevice.leaseId
  const visibleDeviceNetworkState = visibleDevice.networkState
  const visibleDeviceIsDirectWebSerial = isDirectWebSerialDevice(visibleDevice)

  const commitCalibrationState = useCallback((deviceId: string, calibration: CalibrationState) => {
    const normalized = normalizeCalibrationState(calibration)
    calibrationVersionByDeviceRef.current[deviceId] =
      (calibrationVersionByDeviceRef.current[deviceId] ?? 0) + 1
    setCalibrationByDevice((current) => ({
      ...current,
      [deviceId]: normalized,
    }))
  }, [])

  useEffect(() => {
    if (activeView !== 'calibration') {
      return
    }
    let cancelled = false
    const requestVersion = calibrationVersionByDeviceRef.current[visibleDeviceId] ?? 0
    const load = async () => {
      try {
        if (visibleDeviceIsDirectWebSerial) {
          const calibration = await webSerial.getCalibration()
          if (
            !cancelled &&
            (calibrationVersionByDeviceRef.current[visibleDeviceId] ?? 0) === requestVersion
          ) {
            commitCalibrationState(visibleDeviceId, calibration)
          }
          return
        }
        if (lanDeviceBaseUrl) {
          const session = loadLanDeviceSession(lanDeviceBaseUrl)
          if (!session) {
            throw new Error('本机未保存该设备的配对凭据，请重新配对。')
          }
          const calibration = await authorizedLanRequest<CalibrationState>(session, 'calibration')
          if (
            !cancelled &&
            (calibrationVersionByDeviceRef.current[visibleDeviceId] ?? 0) === requestVersion
          ) {
            commitCalibrationState(visibleDeviceId, calibration)
            setFeedback((current) => clearCalibrationLoadWarning(current))
          }
          return
        }
        if (
          visibleDeviceTransport !== 'devd' ||
          !visibleDeviceLeaseId ||
          !devdBaseUrl ||
          visibleDeviceNetworkState === 'error' ||
          visibleDeviceNetworkState === 'timeout'
        ) {
          return
        }
        const calibration = await controlClient.getCalibration(
          devdBaseUrl,
          visibleDeviceId,
          visibleDeviceLeaseId
        )
        if (
          !cancelled &&
          (calibrationVersionByDeviceRef.current[visibleDeviceId] ?? 0) === requestVersion
        ) {
          commitCalibrationState(visibleDeviceId, calibration)
          setFeedback((current) => clearCalibrationLoadWarning(current))
        }
      } catch (error) {
        if (!cancelled) {
          setFeedback({
            title: 'Calibration unavailable',
            detail: errorMessage(error),
            tone: 'warning',
          })
        }
      }
    }
    void load()
    return () => {
      cancelled = true
    }
  }, [
    activeView,
    commitCalibrationState,
    controlClient,
    devdBaseUrl,
    visibleDeviceId,
    visibleDeviceIsDirectWebSerial,
    visibleDeviceLeaseId,
    visibleDeviceNetworkState,
    visibleDeviceTransport,
    lanDeviceBaseUrl,
    webSerial,
  ])
  useEffect(() => {
    if (activeView !== 'calibration') {
      return
    }

    let cancelled = false
    const load = async () => {
      try {
        if (visibleDeviceIsDirectWebSerial) {
          const heaterCurve = await webSerial.getHeaterCurve()
          if (!cancelled) {
            setHeaterCurveByDevice((current) => ({
              ...current,
              [visibleDeviceId]: heaterCurve,
            }))
          }
          return
        }
        if (lanDeviceBaseUrl) {
          const session = loadLanDeviceSession(lanDeviceBaseUrl)
          if (!session) {
            throw new Error('本机未保存该设备的配对凭据，请重新配对。')
          }
          const heaterCurve = await authorizedLanRequest<HeaterCurveState>(session, 'heater-curve')
          if (!cancelled) {
            setHeaterCurveByDevice((current) => ({
              ...current,
              [visibleDeviceId]: heaterCurve,
            }))
            setFeedback((current) => clearCalibrationLoadWarning(current))
          }
          return
        }
        if (
          visibleDeviceTransport !== 'devd' ||
          !visibleDeviceLeaseId ||
          !devdBaseUrl ||
          visibleDeviceNetworkState === 'error' ||
          visibleDeviceNetworkState === 'timeout'
        ) {
          return
        }
        const heaterCurve = await controlClient.getHeaterCurve(
          devdBaseUrl,
          visibleDeviceId,
          visibleDeviceLeaseId
        )
        if (!cancelled) {
          setHeaterCurveByDevice((current) => ({
            ...current,
            [visibleDeviceId]: heaterCurve,
          }))
          setFeedback((current) => clearCalibrationLoadWarning(current))
        }
      } catch (error) {
        if (!cancelled) {
          setFeedback({
            title: 'Heater curve unavailable',
            detail: errorMessage(error),
            tone: 'warning',
          })
        }
      }
    }
    void load()
    return () => {
      cancelled = true
    }
  }, [
    activeView,
    controlClient,
    devdBaseUrl,
    visibleDeviceId,
    visibleDeviceIsDirectWebSerial,
    visibleDeviceLeaseId,
    visibleDeviceNetworkState,
    visibleDeviceTransport,
    lanDeviceBaseUrl,
    webSerial,
  ])
  const scenarioEvents = useMemo(
    () =>
      allowDemoControls
        ? createDemoEventFeed(activeScenario.events, streamTick)
        : activeScenario.events,
    [activeScenario.events, allowDemoControls, streamTick]
  )
  const visibleEvents = useMemo(
    () => [...scenarioEvents, ...actionEvents].slice(-LOG_FEED_SIZE),
    [actionEvents, scenarioEvents]
  )

  const emitEvent = useCallback(
    (source: string, message: string, tone: EventLogEntry['tone'] = 'info') => {
      actionClockRef.current += LOG_FEED_STEP_SECONDS
      setActionEvents((current) =>
        [
          ...current,
          {
            time: allowDemoControls
              ? formatLogTime(actionClockRef.current)
              : formatRuntimeEventTime(new Date()),
            source,
            message,
            tone,
          },
        ].slice(-24)
      )
    },
    [allowDemoControls]
  )

  const handleLanPaired = useCallback(
    async (session: LanDeviceSession, probe: LanProbe) => {
      const target = lanProbeToDeviceTarget(session, probe)
      setFeedback({
        title: '正在获取 LAN 租约',
        detail: `${target.alias} 尚未加入设备列表，等待设备确认控制 lease。`,
        tone: 'info',
      })
      try {
        const lease = await lanRuntime.createLease(session)
        const leasedTarget: DeviceTarget = {
          ...target,
          leaseId: lease.leaseId,
          leaseState: 'active',
        }
        invalidLanCredentialIdentityIdsRef.current.delete(deviceIdentityId(leasedTarget))
        setLanLeasesByDevice((current) => ({ ...current, [target.id]: lease }))
        setPendingDevices((current) => upsertLanDeviceTarget(current, leasedTarget))
        setSelectedDeviceId(target.id)
        setSelectedAddDeviceKind(defaultAddDeviceKind)
        if (navigation) {
          await navigation.navigate({
            kind: 'device',
            deviceId: deviceIdentityId(leasedTarget),
            view: 'dashboard',
          })
        } else {
          await setConsoleView('dashboard')
        }
        setFeedback({
          title: 'LAN 设备已连接',
          detail: `${target.alias} 已取得控制 lease。`,
          tone: 'success',
        })
        emitEvent('lan', `${target.alias} paired and lease acquired`, 'success')
      } catch (error) {
        setFeedback({
          title: 'LAN 租约获取失败',
          detail: error instanceof Error ? error.message : '无法获取 LAN lease。',
          tone: 'warning',
        })
        emitEvent('lan', `${target.alias} lease acquisition failed`, 'warning')
        throw error
      }
    },
    [emitEvent, lanRuntime, navigation, setConsoleView]
  )

  useEffect(() => {
    if (!pendingHeaterConfirmation || !visibleDeviceIsLive) {
      return
    }
    if (pendingHeaterConfirmation.deviceId !== visibleDevice.id) {
      setPendingHeaterConfirmation(null)
      return
    }

    const resolution = resolvePendingHeaterConfirmation(
      pendingHeaterConfirmation,
      visibleDevice,
      heaterConfirmationNowMs(pendingHeaterConfirmation.requestedAtMs, heaterConfirmationNow)
    )
    if (resolution.outcome === 'pending') {
      return
    }

    setPendingHeaterConfirmation(null)
    setFeedback(resolution.feedback)
    emitEvent('heater', resolution.eventMessage, resolution.eventTone)
  }, [
    emitEvent,
    heaterConfirmationNow,
    pendingHeaterConfirmation,
    visibleDevice,
    visibleDeviceIsLive,
  ])

  const applyLocalCalibrationRuntimePatch = useCallback(
    (patch: {
      targetTempC?: number
      selectedPresetSlot?: number
      presetsC?: Array<number | null>
      activeCoolingEnabled?: boolean
      heaterEnabled?: boolean
      manualPpsEnabled?: boolean
      manualPpsMv?: number
      faultAttentionAcknowledged?: boolean
      calibration?: CalibrationControlRequest
    }) => {
      const calibrationPatch = patch.calibration
      if (!calibrationPatch) {
        return
      }
      setCalibrationRuntimeByDevice((current) => {
        const base = current[visibleDevice.id] ?? visibleDevice.calibration
        const next = applyLocalCalibrationRuntimeRequest(base, calibrationPatch)
        return {
          ...current,
          [visibleDevice.id]: next,
        }
      })
    },
    [visibleDevice.calibration, visibleDevice.id]
  )

  const reconcileDirectLanStaleWrite = useCallback(
    async <T,>(
      error: unknown,
      refresh: () => Promise<T>,
      applyRefreshed: (refreshed: T) => void
    ) => {
      const refreshed = await reconcileStaleLanWrite(error, refresh)
      if (refreshed === null) {
        return false
      }
      applyRefreshed(refreshed)
      return true
    },
    []
  )

  const configureLiveRuntime = useCallback(
    async (
      patch: {
        targetTempC?: number
        selectedPresetSlot?: number
        presetsC?: Array<number | null>
        activeCoolingEnabled?: boolean
        heaterEnabled?: boolean
        manualPpsEnabled?: boolean
        manualPpsMv?: number
        faultAttentionAcknowledged?: boolean
        calibration?: CalibrationControlRequest
      },
      failureMessage: string
    ) => {
      const blockedReason = deviceControlBlockReason(visibleDevice)
      if (blockedReason) {
        setFeedback({
          title: 'Runtime update blocked',
          detail: blockedReason,
          tone: 'warning',
        })
        emitEvent('devd', 'runtime update blocked by transport state', 'warning')
        return false
      }

      if (isDirectWebSerialDevice(visibleDevice)) {
        const updated = await webSerial.configureRuntime(patch)
        if (!updated) {
          setFeedback({
            title: 'Runtime update failed',
            detail: webSerial.error ?? failureMessage,
            tone: 'warning',
          })
          emitEvent('webserial', failureMessage, 'warning')
        }
        return updated
      }

      if (isDirectLanDevice(visibleDevice)) {
        const session = loadLanDeviceSession(visibleDevice.baseUrl)
        if (!session || !visibleDevice.leaseId) {
          setFeedback({
            title: 'LAN runtime update blocked',
            detail: '设备未取得有效 LAN lease，请重新选择或配对。',
            tone: 'warning',
          })
          return false
        }
        try {
          const preflight = await lanRuntime.probeDevice(session, undefined, 'serial')
          setPendingDevices((current) =>
            current.map((device) =>
              device.id === visibleDevice.id ? applyLanStatus(device, preflight.status) : device
            )
          )
          const updatedStatus = await lanRuntime.writeRuntime(session, visibleDevice.leaseId, patch)
          setPendingDevices((current) =>
            current.map((device) =>
              device.id === visibleDevice.id ? applyLanStatus(device, updatedStatus) : device
            )
          )
          return true
        } catch (error) {
          const staleWrite =
            error instanceof ControlPlaneClientError && error.code === 'stale_write'
          if (staleWrite) {
            await reconcileDirectLanStaleWrite(
              error,
              () => lanRuntime.readStatus(session),
              (refreshed) => {
                setPendingDevices((current) =>
                  current.map((device) =>
                    device.id === visibleDevice.id ? applyLanStatus(device, refreshed) : device
                  )
                )
              }
            )
          }
          const detail = staleWrite
            ? '设备控制状态已变化，已读取最新状态；请确认后重新提交。'
            : error instanceof Error
              ? error.message
              : failureMessage
          setFeedback({
            title: 'LAN runtime update failed',
            detail,
            tone: 'warning',
          })
          emitEvent('lan', failureMessage, 'warning')
          return false
        }
      }

      if (visibleDevice.transport !== 'devd' || !visibleDevice.leaseId || !devdBaseUrl) {
        applyLocalCalibrationRuntimePatch(patch)
        return false
      }

      try {
        await controlClient.configureRuntime(devdBaseUrl, visibleDevice.id, {
          leaseId: visibleDevice.leaseId,
          ...patch,
        })
        if (patch.calibration) {
          applyLocalCalibrationRuntimePatch(patch)
        }
        return true
      } catch (error) {
        const detail = error instanceof Error ? error.message : failureMessage
        setFeedback({
          title: 'Runtime update failed',
          detail,
          tone: 'warning',
        })
        emitEvent('devd', failureMessage, 'warning')
        return false
      }
    },
    [
      applyLocalCalibrationRuntimePatch,
      controlClient,
      devdBaseUrl,
      emitEvent,
      lanRuntime.probeDevice,
      lanRuntime.readStatus,
      lanRuntime.writeRuntime,
      reconcileDirectLanStaleWrite,
      visibleDevice,
      webSerial,
    ]
  )

  const configureWifi = useCallback(
    async (op: 'set' | 'clear', draft?: WifiNetworkSettingsDraft) => {
      const rejectWifiConfiguration = (detail: string): never => {
        emitEvent('devd', 'wifi configuration blocked by USB lease state', 'warning')
        throw new Error(detail)
      }

      if (visibleDevice.transport !== 'devd') {
        rejectWifiConfiguration('WiFi 设置仅能通过已连接的 USB / devd 目标写入。')
      }
      const wifiDevdBaseUrl = devdBaseUrl
      if (!wifiDevdBaseUrl) {
        return rejectWifiConfiguration('WiFi 设置仅能通过已连接的 USB / devd 目标写入。')
      }
      if (!visibleDevice.capabilities.includes('wifi_config')) {
        rejectWifiConfiguration('当前设备不支持 WiFi 设置。')
      }
      if (!visibleDevice.capabilities.includes('wifi_state_v2')) {
        rejectWifiConfiguration('当前设备固件需要 WiFi 状态协议更新后才能提交设置。')
      }
      const wifiLeaseId = visibleDevice.leaseId
      if (!wifiLeaseId || visibleDevice.leaseState !== 'active') {
        return rejectWifiConfiguration('正在获取 USB 租约，请稍候再提交 WiFi 设置。')
      }
      if (visibleDevice.severity === 'offline') {
        rejectWifiConfiguration('目标设备当前离线。')
      }

      try {
        const network = await controlClient.configureWifi(wifiDevdBaseUrl, visibleDevice.id, {
          leaseId: wifiLeaseId,
          op,
          ...(draft
            ? {
                ssid: draft.ssid,
                ...(draft.password === undefined ? {} : { password: draft.password }),
              }
            : {}),
        })
        setWifiSnapshotsByDevice((current) => ({
          ...current,
          [visibleDevice.id]: network,
        }))
        emitEvent(
          'devd',
          op === 'set'
            ? 'wifi configuration submitted through USB bridge'
            : 'saved wifi cleared through USB bridge',
          'success'
        )
        return network
      } catch (error) {
        emitEvent('devd', 'wifi configuration rejected by USB bridge', 'warning')
        throw error
      }
    },
    [controlClient, devdBaseUrl, emitEvent, visibleDevice]
  )

  const handleWifiSave = useCallback(
    (draft: WifiNetworkSettingsDraft) => configureWifi('set', draft),
    [configureWifi]
  )

  const handleWifiClear = useCallback(() => configureWifi('clear'), [configureWifi])

  useEffect(() => {
    const timer = window.setInterval(() => {
      setCurrentTempByDevice((current) => {
        if (!selectedDevice || visibleDevice.severity === 'offline') {
          return current
        }
        if (selectedDevice.transport === 'devd' || isDirectWebSerialDevice(selectedDevice)) {
          return current
        }

        const baseTemp = current[visibleDevice.id] ?? selectedDevice.currentTempC
        const targetTemp = targetTempByDevice[visibleDevice.id] ?? selectedDevice.targetTempC
        const fanState = fanPolicyByDevice[visibleDevice.id] ?? selectedDevice.fanState
        const isHeld = heaterHeldByDevice[visibleDevice.id]
        const delta = targetTemp - baseTemp

        if (Math.abs(delta) < 0.2) {
          return current
        }

        const step =
          delta > 0 && !isHeld
            ? Math.min(7, Math.max(0.4, Math.abs(delta) * 0.08))
            : -Math.min(
                fanState === 'RUN' ? 10 : fanState === 'AUTO' ? 6 : 3,
                Math.max(0.3, Math.abs(delta) * 0.06)
              )

        return {
          ...current,
          [visibleDevice.id]: Number((baseTemp + step).toFixed(1)),
        }
      })
    }, 1500)

    return () => window.clearInterval(timer)
  }, [
    fanPolicyByDevice,
    heaterHeldByDevice,
    selectedDevice,
    targetTempByDevice,
    visibleDevice.id,
    visibleDevice.severity,
  ])

  useEffect(() => {
    if (flashRun.status !== 'running' && flashRun.status !== 'flashing') {
      return
    }

    const timer = window.setInterval(() => {
      setFlashRun((current) => {
        if (current.status !== 'running' && current.status !== 'flashing') {
          return current
        }

        return {
          ...current,
          progress: Math.min(current.status === 'flashing' ? 92 : 100, current.progress + 14),
        }
      })
    }, 420)

    return () => window.clearInterval(timer)
  }, [flashRun.status])

  useEffect(() => {
    if (
      flashRun.status !== 'running' ||
      flashRun.progress < 100 ||
      flashCompletionEmittedRef.current
    ) {
      return
    }

    flashCompletionEmittedRef.current = true
    setFeedback({
      title: 'Dry-run passed',
      detail: 'Artifact hash and target profile match this device.',
      tone: 'success',
    })
    emitEvent('flash', `${selectedArtifact?.version ?? 'artifact'} dry-check passed`, 'success')
    setFlashRun({ status: 'passed', progress: 100 })
  }, [emitEvent, flashRun.progress, flashRun.status, selectedArtifact?.version])

  const dismissCalibrationLeaveGuard = useCallback(() => {
    setCalibrationLeaveGuard(null)
  }, [])

  const cancelCalibrationLeaveGuard = useCallback(() => {
    setCalibrationLeaveGuard((current) => {
      current?.cancelAction?.()
      return null
    })
  }, [])

  const requestCalibrationLeave = useCallback(
    async (
      request: CalibrationLeaveRequest,
      continueAction: () => void | Promise<void>
    ): Promise<boolean> => {
      let shouldBlock = false

      switch (request.reason) {
        case 'view-change':
        case 'add-device-flow':
          shouldBlock = shouldBlockCalibrationViewChange(
            visibleRuntimeCalibration.mode,
            activeView,
            request.nextView ?? 'dashboard'
          )
          break
        case 'device-change':
          shouldBlock = shouldBlockCalibrationDeviceChange(
            visibleRuntimeCalibration.mode,
            activeView
          )
          break
        case 'workspace-tab-change':
          shouldBlock = shouldBlockCalibrationWorkspaceTabChange(
            visibleRuntimeCalibration.mode,
            visibleCalibrationWorkspaceTab,
            request.nextWorkspaceTab ?? visibleCalibrationWorkspaceTab
          )
          break
      }

      if (!shouldBlock) {
        await continueAction()
        return true
      }

      setCalibrationLeaveGuard({
        ...request,
        continueAction,
      })
      setFeedback({
        title: '请先关闭校准控制',
        detail: `${calibrationModeLabel(visibleCalibrationWorkspaceTab)}仍在运行，离开前请先关闭开关。`,
        tone: 'warning',
      })
      return false
    },
    [activeView, visibleCalibrationWorkspaceTab, visibleRuntimeCalibration.mode]
  )

  const handleDeviceChange = (deviceId: string, deviceOverride?: DeviceTarget) => {
    if (navigation) {
      if (deviceId === ADD_DEVICE_VALUE) {
        void requestCalibrationLeave(
          {
            reason: 'add-device-flow',
            nextLabel: '添加设备',
            nextView: 'add-device',
          },
          () => navigation.navigate({ kind: 'add-device' })
        )
        return
      }
      const nextDevice = deviceOverride ?? deviceOptions.find((device) => device.id === deviceId)
      if (!nextDevice) return
      const identityId = deviceIdentityId(nextDevice)
      const connection =
        routeDeviceChoices
          .find((choice) => choice.identityId === identityId)
          ?.connections.find((candidate) => candidate.target.id === nextDevice.id) ??
        deviceConnectionOptions(nextDevice, { allowDemoControls })[0]
      void requestCalibrationLeave(
        {
          reason: 'device-change',
          nextLabel: nextDevice.alias,
        },
        () => {
          if (connection) {
            setRequestedConnectionByIdentity((current) => ({
              ...current,
              [identityId]: { kind: connection.kind, targetId: connection.target.id },
            }))
            setRouteFallbackKind(undefined)
          }
          const currentState = navigation.state
          return navigation.navigate(
            currentState.kind === 'device'
              ? { ...currentState, deviceId: identityId }
              : { kind: 'device', deviceId: identityId, view: 'dashboard' }
          )
        }
      )
      return
    }

    if (deviceId === ADD_DEVICE_VALUE) {
      void requestCalibrationLeave(
        {
          reason: 'add-device-flow',
          nextLabel: '添加设备',
          nextView: 'add-device',
        },
        () => {
          void setConsoleView('add-device')
          setSelectedAddDeviceKind(defaultAddDeviceKind)
          setFlashRun({ status: 'idle', progress: 0 })
          flashCompletionEmittedRef.current = false
          setFeedback({
            title: 'Add device',
            detail: 'Choose WiFi, Web Serial, or Bridge from the add device page.',
            tone: 'info',
          })
        }
      )
      return
    }

    const nextDevice = deviceOptions.find((device) => device.id === deviceId)

    void requestCalibrationLeave(
      {
        reason: 'device-change',
        nextLabel: nextDevice?.alias ?? '切换设备',
      },
      () => {
        if (
          nextDevice &&
          isDirectWebSerialDevice(nextDevice) &&
          (webSerial.state !== 'connected' || webSerial.deviceId !== nextDevice.id)
        ) {
          setSelectedAddDeviceKind('web-serial')
          setFeedback({
            title: '正在验证 Web Serial 设备',
            detail: `正在通过浏览器已授权串口核对 ${nextDevice.alias} 的设备 ID。`,
            tone: 'info',
          })
          emitEvent('webserial', `verifying remembered device ${nextDevice.alias}`, 'info')
          void handleWebSerialConnect({ replaceExisting: true }).then((connected) => {
            if (connected) void setConsoleView('dashboard')
          })
          return
        }
        if (nextDevice && !isDirectWebSerialDevice(nextDevice)) {
          setSelectedAddDeviceKind(defaultAddDeviceKind)
        }
        if (nextDevice && shouldReacquireLanLeaseOnExplicitSelection(nextDevice)) {
          setPendingDevices((current) =>
            current.map((device) =>
              device.id === nextDevice.id
                ? { ...device, leaseState: 'none', transportIssue: undefined }
                : device
            )
          )
        }
        setSelectedDeviceId(deviceId)
        setFlashRun({ status: 'idle', progress: 0 })
        flashCompletionEmittedRef.current = false

        if (!nextDevice) {
          return
        }

        setFeedback({
          title: `${nextDevice.alias} selected`,
          detail: `${transportLabels[nextDevice.transport]} target loaded with ${nextDevice.firmware}.`,
          tone: nextDevice.severity === 'nominal' ? 'info' : 'warning',
        })
        emitEvent(
          'target',
          `${nextDevice.alias} selected`,
          nextDevice.severity === 'offline' ? 'warning' : 'info'
        )
      }
    )
  }

  const handleAddDevice = async (
    kind: AddDeviceKind,
    { showPendingDashboard = true }: { showPendingDashboard?: boolean } = {}
  ) => {
    setFlashRun({ status: 'idle', progress: 0 })
    flashCompletionEmittedRef.current = false

    if (kind === 'web-serial' && !allowDemoControls) {
      setFeedback({
        title: '正在连接 Web Serial',
        detail: '正在等待浏览器选择串口；连接超时后会自动结束并允许重试。',
        tone: 'info',
      })
      emitEvent('webserial', 'waiting for browser Web Serial port selection', 'info')
      const connected = await handleWebSerialConnect({
        forcePortSelection: true,
        replaceExisting: true,
      })
      if (connected) {
        void setConsoleView('dashboard')
      }
      return
    }

    const nextDevice = createPendingDevice(kind)
    setPendingDevices((current) =>
      current.some((device) => device.id === nextDevice.id) ? current : [...current, nextDevice]
    )
    setSelectedDeviceId(nextDevice.id)
    if (!navigation) {
      void setConsoleView(showPendingDashboard ? 'dashboard' : 'add-device')
    }
    setFeedback({
      title: `${nextDevice.alias} added`,
      detail:
        nextDevice.transportIssue ?? `${transportLabels[nextDevice.transport]} target pending.`,
      tone: 'warning',
    })
    emitEvent('target', `${nextDevice.alias} added from target selector`, 'warning')
  }

  const handleAddDeviceChoice = async (
    kind: AddDeviceKind,
    { showPendingDashboard = true }: { showPendingDashboard?: boolean } = {}
  ) => {
    setSelectedAddDeviceKind(kind)
    if (kind === 'wifi') {
      setFeedback({
        title: 'WiFi / LAN',
        detail: '输入设备的私有 HTTP 地址以开始匿名连接。',
        tone: 'info',
      })
      return
    }
    if (kind === 'bridge') {
      setFeedback({
        title: 'DEVD 桥接',
        detail: '先选择 USB 或 WiFi / LAN，再明确选择一台 DEVD 已发现的设备。',
        tone: 'info',
      })
      return
    }
    await handleAddDevice(kind, { showPendingDashboard })
  }

  const handleBridgeTargetSelect = (device: DeviceTarget) => {
    setPendingDevices((current) => upsertLanDeviceTarget(current, device))
    handleDeviceChange(device.id, device)
    setSelectedAddDeviceKind(defaultAddDeviceKind)
    if (!navigation) {
      void setConsoleView('dashboard')
    }
  }

  const handleQuickAddDevice = async (kind: AddDeviceKind) => {
    await requestCalibrationLeave(
      {
        reason: 'add-device-flow',
        nextLabel: addDeviceOptions.find((option) => option.kind === kind)?.label ?? '添加设备',
        nextView: 'add-device',
      },
      async () => {
        void setConsoleView('add-device')
        await handleAddDeviceChoice(kind, { showPendingDashboard: false })
      }
    )
  }

  const handleGuardedViewChange = useCallback(
    (nextView: ConsoleView) => {
      if (nextView === activeView) {
        dismissCalibrationLeaveGuard()
        return
      }

      if (navigation) {
        void setConsoleView(nextView)
        return
      }

      void requestCalibrationLeave(
        {
          reason: 'view-change',
          nextLabel: consoleViewLabel(nextView),
          nextView,
        },
        () => {
          dismissCalibrationLeaveGuard()
          void setConsoleView(nextView)
        }
      )
    },
    [activeView, dismissCalibrationLeaveGuard, navigation, requestCalibrationLeave, setConsoleView]
  )

  const handleGuardedWorkspaceTabChange = useCallback(
    (nextTab: CalibrationWorkspaceTab) => {
      if (nextTab === visibleCalibrationWorkspaceTab) {
        dismissCalibrationLeaveGuard()
        return
      }

      if (navigation) {
        void setWorkspaceTab(nextTab)
        return
      }

      void requestCalibrationLeave(
        {
          reason: 'workspace-tab-change',
          nextLabel: calibrationModeLabel(nextTab),
          nextWorkspaceTab: nextTab,
        },
        () => {
          dismissCalibrationLeaveGuard()
          void setWorkspaceTab(nextTab)
        }
      )
    },
    [
      dismissCalibrationLeaveGuard,
      navigation,
      requestCalibrationLeave,
      setWorkspaceTab,
      visibleCalibrationWorkspaceTab,
    ]
  )

  async function handleWebSerialConnect(options?: {
    forcePortSelection?: boolean
    replaceExisting?: boolean
    preauthorizedOnly?: boolean
  }) {
    const connected = await webSerial.connect(options)
    setFeedback(
      connected
        ? {
            title: 'Web Serial connected',
            detail: 'Browser direct USB JSONL control is active.',
            tone: 'success',
          }
        : {
            title: 'Web Serial unavailable',
            detail: webSerial.error ?? 'Browser direct USB control could not be opened.',
            tone: 'warning',
          }
    )
    emitEvent(
      'webserial',
      connected ? 'browser direct USB control connected' : 'browser direct USB control failed',
      connected ? 'success' : 'warning'
    )
    return connected
  }

  const handleTargetTempChange = (nextTargetTemp: number) => {
    const clampedTarget = clampTargetTemp(nextTargetTemp)
    const deviceId = visibleDevice.id
    const blockedReason = deviceControlBlockReason(visibleDevice)
    if (blockedReason) {
      setFeedback({
        title: visibleDevice.leaseState === 'conflict' ? '目标温度更新被阻止' : '硬件连接受阻',
        detail: blockedReason,
        tone: 'warning',
      })
      emitEvent('devd', 'target temperature update blocked by transport state', 'warning')
      return
    }

    setTargetTempByDevice((current) => ({
      ...current,
      [deviceId]: clampedTarget,
    }))

    if (visibleDeviceIsLive) {
      const nextVersion = (targetTempCommitVersionRef.current[deviceId] ?? 0) + 1
      targetTempCommitVersionRef.current[deviceId] = nextVersion
      const existingTimer = targetTempCommitTimersRef.current[deviceId]
      if (existingTimer) {
        window.clearTimeout(existingTimer)
      }
      targetTempCommitTimersRef.current[deviceId] = window.setTimeout(async () => {
        delete targetTempCommitTimersRef.current[deviceId]
        const liveUpdated = await configureLiveRuntime(
          { targetTempC: clampedTarget },
          'target temperature update was not accepted by devd'
        )
        if (targetTempCommitVersionRef.current[deviceId] !== nextVersion) {
          return
        }
        if (liveUpdated) {
          setFeedback({
            title: 'Target updated',
            detail: `${visibleDevice.alias} target is now ${formatTemp(clampedTarget)}.`,
            tone: 'success',
          })
          emitEvent(
            'thermal',
            `target temperature updated to ${formatTemp(clampedTarget)}`,
            'success'
          )
          return
        }
        setTargetTempByDevice((current) => {
          const next = { ...current }
          delete next[deviceId]
          return next
        })
      }, 180)
    }

    if (!visibleDeviceIsLive) {
      setFeedback({
        title: 'Target updated',
        detail: `${visibleDevice.alias} target is now ${formatTemp(clampedTarget)}.`,
        tone: 'success',
      })
      emitEvent('thermal', `target temperature updated to ${formatTemp(clampedTarget)}`, 'success')
    }
  }

  const handleFanPolicyChange = async (fanState: DeviceTarget['fanState']) => {
    if (fanState === 'RUN') {
      return
    }
    const liveUpdated = await configureLiveRuntime(
      { activeCoolingEnabled: fanState !== 'OFF' },
      'fan policy update was not accepted by devd'
    )
    if (visibleDeviceIsLive && !liveUpdated) {
      return
    }
    setFanPolicyByDevice((current) => ({
      ...current,
      [visibleDevice.id]: fanState,
    }))
    setFeedback({
      title: 'Fan policy updated',
      detail: `${visibleDevice.alias} fan policy is now ${fanState}.`,
      tone: fanState === 'OFF' ? 'warning' : 'success',
    })
    emitEvent('cooling', `fan policy updated to ${fanState}`, 'info')
  }

  const handleManualPpsApply = async (millivolts: number) => {
    const boundedMv = clampPpsMv(millivolts, visibleDevice)
    if (boundedMv !== millivolts) {
      setFeedback({
        title: 'PPS 申请被拒绝',
        detail: `${visibleDevice.alias} 只接受实时 capability 范围内、且满足 100mV 步进的 PPS 电压请求。`,
        tone: 'warning',
      })
      emitEvent('pd', 'manual PPS request rejected before submit', 'warning')
      return
    }
    const liveUpdated = await configureLiveRuntime(
      { manualPpsEnabled: true, manualPpsMv: boundedMv },
      'manual PPS update was not accepted by devd'
    )
    if (visibleDeviceIsLive && !liveUpdated) {
      return
    }
    if (!visibleDeviceIsLive) {
      setManualPpsByDevice((current) => ({
        ...current,
        [visibleDevice.id]: { enabled: true, mv: boundedMv },
      }))
    }
    setFeedback({
      title: 'PPS 已申请',
      detail: `${visibleDevice.alias} 正在申请 ${formatVolts(boundedMv)}。`,
      tone: 'warning',
    })
    emitEvent('pd', `manual PPS set to ${formatVolts(boundedMv)}`, 'warning')
  }

  const handleManualPpsClear = async () => {
    const liveUpdated = await configureLiveRuntime(
      { manualPpsEnabled: false },
      'manual PPS clear was not accepted by devd'
    )
    if (visibleDeviceIsLive && !liveUpdated) {
      return
    }
    if (!visibleDeviceIsLive) {
      setManualPpsByDevice((current) => ({
        ...current,
        [visibleDevice.id]: { enabled: false, mv: null },
      }))
    }
    setFeedback({
      title: 'PPS 已关闭',
      detail: `${visibleDevice.alias} 已恢复自动供电控制。`,
      tone: 'success',
    })
    emitEvent('pd', 'manual PPS override cleared', 'success')
  }

  const handlePresetSlotChange = async (presetIndex: number) => {
    const presetIsEnabled = visiblePresetEnabled[presetIndex] ?? true
    setSelectedPresetByDevice((current) => ({
      ...current,
      [visibleDevice.id]: presetIndex,
    }))
    const liveUpdated = await configureLiveRuntime(
      { selectedPresetSlot: presetIndex },
      'preset slot update was not accepted by devd'
    )
    if (visibleDeviceIsLive && !liveUpdated) {
      setSelectedPresetByDevice((current) => {
        const next = { ...current }
        delete next[visibleDevice.id]
        return next
      })
      return
    }
    setFeedback({
      title: `Preset M${presetIndex + 1} selected`,
      detail: presetIsEnabled
        ? `${formatTemp(visiblePresetTemps[presetIndex])} is ready for ${visibleDevice.alias}.`
        : `Preset M${presetIndex + 1} is disabled.`,
      tone: presetIsEnabled ? 'info' : 'warning',
    })
    emitEvent('preset', `selected M${presetIndex + 1}`, 'info')
  }

  const handlePresetTempChange = async (nextTempC: number) => {
    const clampedTemp = clampTargetTemp(nextTempC)
    const nextPresetValues = [...visiblePresetValues]
    nextPresetValues[selectedPresetIndex] = clampedTemp
    const liveUpdated = await configureLiveRuntime(
      { selectedPresetSlot: selectedPresetIndex, presetsC: nextPresetValues },
      'preset temperature update was not accepted by devd'
    )
    if (visibleDeviceIsLive && !liveUpdated) {
      return
    }
    if (!visibleDeviceIsLive) {
      setPresetTempsByDevice((current) => {
        const nextTemps = [...(current[visibleDevice.id] ?? PRESET_TEMPS_C)]
        nextTemps[selectedPresetIndex] = clampedTemp

        return { ...current, [visibleDevice.id]: nextTemps }
      })
      setPresetEnabledByDevice((current) => {
        const nextEnabledState = [...(current[visibleDevice.id] ?? PRESET_ENABLED)]
        nextEnabledState[selectedPresetIndex] = true

        return { ...current, [visibleDevice.id]: nextEnabledState }
      })
    }
    setFeedback({
      title: `Preset M${selectedPresetIndex + 1} updated`,
      detail: `Preset temperature is now ${formatTemp(clampedTemp)}.`,
      tone: 'success',
    })
    emitEvent(
      'preset',
      `M${selectedPresetIndex + 1} updated to ${formatTemp(clampedTemp)}`,
      'success'
    )
  }

  const handlePresetEnabledChange = async (nextEnabled: boolean) => {
    const nextPresetValues = [...visiblePresetValues]
    nextPresetValues[selectedPresetIndex] = nextEnabled
      ? visiblePresetTemps[selectedPresetIndex]
      : null
    const liveUpdated = await configureLiveRuntime(
      { selectedPresetSlot: selectedPresetIndex, presetsC: nextPresetValues },
      'preset enabled update was not accepted by devd'
    )
    if (visibleDeviceIsLive && !liveUpdated) {
      return
    }
    if (!visibleDeviceIsLive) {
      setPresetEnabledByDevice((current) => {
        const nextEnabledState = [...(current[visibleDevice.id] ?? PRESET_ENABLED)]
        nextEnabledState[selectedPresetIndex] = nextEnabled

        return { ...current, [visibleDevice.id]: nextEnabledState }
      })
    }
    setFeedback({
      title: `Preset M${selectedPresetIndex + 1} ${nextEnabled ? 'enabled' : 'disabled'}`,
      detail: nextEnabled
        ? `${formatTemp(visiblePresetTemps[selectedPresetIndex])} can be used as a live target.`
        : 'This preset is hidden from quick target use.',
      tone: nextEnabled ? 'success' : 'warning',
    })
    emitEvent(
      'preset',
      `M${selectedPresetIndex + 1} ${nextEnabled ? 'enabled' : 'disabled'}`,
      nextEnabled ? 'success' : 'warning'
    )
  }

  const handleHeaterHoldToggle = async () => {
    const nextHeld = visibleDeviceIsLive
      ? visibleDevice.heaterEnabled
      : !heaterHeldByDevice[visibleDevice.id]
    const nextHeaterEnabled = !nextHeld
    const liveUpdated = await configureLiveRuntime(
      { heaterEnabled: nextHeaterEnabled },
      'heater hold update was not accepted by devd'
    )
    if (visibleDeviceIsLive && !liveUpdated) {
      return
    }
    if (visibleDeviceIsLive) {
      const requestedAtMs = Date.now()
      setHeaterConfirmationNow(requestedAtMs)
      setPendingHeaterConfirmation({
        deviceId: visibleDevice.id,
        requestedEnabled: nextHeaterEnabled,
        requestedAtMs,
      })
      setFeedback(createPendingHeaterFeedback(nextHeaterEnabled))
      return
    }
    setHeaterHeldByDevice((current) => ({
      ...current,
      ...(visibleDeviceIsLive ? {} : { [visibleDevice.id]: nextHeld }),
    }))
    setFeedback({
      title: nextHeaterEnabled ? 'Heater resumed' : 'Heater held',
      detail: nextHeaterEnabled
        ? 'Heater output follows the target temperature again.'
        : 'Heater output is disabled until resumed again.',
      tone: nextHeaterEnabled ? 'success' : 'warning',
    })
    emitEvent(
      'heater',
      nextHeaterEnabled ? 'heater output resumed' : 'heater output held at 0%',
      nextHeaterEnabled ? 'success' : 'warning'
    )
  }

  const handleFaultAttentionAcknowledge = async () => {
    if (!visibleDeviceIsLive) {
      setFeedback({
        title: '消告警仅支持在线硬件',
        detail: '请在 live devd 或浏览器 Web Serial 连接下确认告警。',
        tone: 'warning',
      })
      emitEvent('thermal', 'fault attention acknowledge blocked outside live mode', 'warning')
      return
    }
    const liveUpdated = await configureLiveRuntime(
      { faultAttentionAcknowledged: true },
      'fault attention acknowledge was not accepted by devd'
    )
    if (!liveUpdated) {
      return
    }
    setFeedback({
      title: '告警已确认',
      detail: `${visibleDevice.alias} 已清除待确认告警提醒。`,
      tone: 'success',
    })
    emitEvent('thermal', 'fault attention acknowledged', 'success')
  }

  const handleStartDryRun = async () => {
    if (
      visibleDevice.severity === 'offline' ||
      selectedArtifact?.compatibility === 'blocked' ||
      !selectedArtifact
    ) {
      setFeedback({
        title: 'Dry-run unavailable',
        detail:
          visibleDevice.severity === 'offline'
            ? `${visibleDevice.alias} is offline.`
            : `${selectedArtifact?.version ?? 'Artifact'} is not compatible with this target.`,
        tone: 'warning',
      })
      emitEvent('flash', 'dry-check blocked before start', 'warning')
      return
    }

    if (
      visibleDevice.transport === 'devd' &&
      (!visibleDevice.leaseId ||
        !devdBaseUrl ||
        visibleDevice.leaseState === 'conflict' ||
        visibleDevice.leaseState === 'expired')
    ) {
      setFeedback({
        title: 'Dry-run lease required',
        detail: 'Firmware recovery requires an active devd lease for the native target.',
        tone: 'warning',
      })
      emitEvent('flash', 'dry-check blocked: missing devd lease', 'warning')
      return
    }

    flashCompletionEmittedRef.current = false
    setFlashRun({ status: 'running', progress: 0 })
    setFeedback({
      title: 'Dry-run started',
      detail: `${selectedArtifact.version} hash and target profile are being checked.`,
      tone: selectedArtifact.compatibility === 'warning' ? 'warning' : 'info',
    })
    emitEvent('flash', `${selectedArtifact.version} dry-check started`, 'info')

    if (!devdBaseUrl || !selectedArtifact.files?.length) {
      return
    }

    try {
      const result = await controlClient.verifyArtifact(
        devdBaseUrl,
        artifactToManifest(selectedArtifact)
      )
      if (!result.verified) {
        setFlashRun({ status: 'idle', progress: 0 })
        setFeedback({
          title: 'Dry-run failed',
          detail: `${selectedArtifact.version} failed local file verification.`,
          tone: 'warning',
        })
        emitEvent('flash', `${selectedArtifact.version} verification failed`, 'warning')
        return
      }

      if (visibleDevice.transport === 'devd' && visibleDevice.leaseId) {
        const dryRun = await controlClient.flashDevice(devdBaseUrl, visibleDevice.id, {
          leaseId: visibleDevice.leaseId,
          artifact: artifactToManifest(selectedArtifact),
          dryRun: true,
        })
        emitEvent('flash', `${dryRun.artifactId} dry-run registered by devd`, 'success')
      }

      flashCompletionEmittedRef.current = true
      setFlashRun({ status: 'passed', progress: 100 })
      setFeedback({
        title: 'Dry-run passed',
        detail: `${selectedArtifact.version} verified ${result.files.length} local file${result.files.length === 1 ? '' : 's'}.`,
        tone: 'success',
      })
      emitEvent('flash', `${selectedArtifact.version} verified by devd`, 'success')
    } catch (error) {
      const detail = error instanceof Error ? error.message : 'Artifact verification failed.'
      setFlashRun({ status: 'idle', progress: 0 })
      setFeedback({
        title: 'Dry-run failed',
        detail,
        tone: 'warning',
      })
      emitEvent('flash', 'devd artifact verification failed', 'warning')
    }
  }

  const handleStartFlash = async () => {
    if (
      !selectedArtifact ||
      selectedArtifact.compatibility === 'blocked' ||
      visibleDevice.transport !== 'devd' ||
      !visibleDevice.leaseId ||
      !devdBaseUrl ||
      flashRun.status !== 'passed'
    ) {
      setFeedback({
        title: 'Flash unavailable',
        detail:
          'Real flash requires a devd target, active lease, compatible artifact, and passed dry-run.',
        tone: 'warning',
      })
      emitEvent('flash', 'real flash blocked before start', 'warning')
      return
    }

    setFlashRun({ status: 'flashing', progress: 8 })
    setFeedback({
      title: 'Flash started',
      detail: `${selectedArtifact.version} is being written by devd.`,
      tone: 'warning',
    })
    emitEvent('flash', `${selectedArtifact.version} flash command submitted`, 'warning')

    try {
      const result = await controlClient.flashDevice(devdBaseUrl, visibleDevice.id, {
        leaseId: visibleDevice.leaseId,
        artifact: artifactToManifest(selectedArtifact),
        dryRun: false,
        confirm: 'FLASH',
      })
      setFlashRun({ status: 'flashed', progress: 100 })
      setFeedback({
        title: 'Flash completed',
        detail: result.message,
        tone: 'success',
      })
      emitEvent('flash', `${result.artifactId} flashed by devd`, 'success')
    } catch (error) {
      const detail = error instanceof Error ? error.message : 'Real flash failed.'
      setFlashRun({ status: 'passed', progress: 100 })
      setFeedback({
        title: 'Flash blocked',
        detail,
        tone: 'warning',
      })
      emitEvent('flash', 'devd real flash failed or was blocked', 'warning')
    }
  }

  const handleArtifactChange = (artifactId: string) => {
    const nextArtifact = activeScenario.artifacts.find((artifact) => artifact.id === artifactId)

    setArtifactByDevice((current) => ({
      ...current,
      [visibleDevice.id]: artifactId,
    }))
    setFlashRun({ status: 'idle', progress: 0 })
    flashCompletionEmittedRef.current = false

    if (!nextArtifact) {
      return
    }

    const blocked = nextArtifact.compatibility === 'blocked'
    setFeedback({
      title: `${nextArtifact.version} selected`,
      detail: blocked
        ? 'This artifact does not match the active target.'
        : `${nextArtifact.profile} is ready for a dry-check.`,
      tone: blocked ? 'warning' : 'info',
    })
    emitEvent('artifact', `${nextArtifact.version} selected`, blocked ? 'warning' : 'info')
  }

  const setCalibrationReference = (channel: CalibrationChannel, value: number) => {
    setCalibrationRefsByDevice((current) => {
      const existing = current[visibleDevice.id] ?? visibleCalibrationRefs
      return {
        ...current,
        [visibleDevice.id]:
          channel === 'rtd_adc' ? { ...existing, rtdTempC: value } : { ...existing, vinMv: value },
      }
    })
  }

  const updateCalibrationDraft = async (request: Omit<CalibrationConfigRequest, 'leaseId'>) => {
    if (isDirectWebSerialDevice(visibleDevice)) {
      const calibration = await webSerial.configureCalibration(request)
      commitCalibrationState(visibleDevice.id, calibration)
      return
    }
    if (isDirectLanDevice(visibleDevice)) {
      const session = loadLanDeviceSession(visibleDevice.baseUrl)
      if (!session || !visibleDevice.leaseId) {
        throw new Error('设备未取得有效 LAN lease，请重新选择或配对。')
      }
      try {
        const calibration = await authorizedLanRequest<CalibrationState>(
          session,
          'calibration',
          'PUT',
          request,
          visibleDevice.leaseId
        )
        commitCalibrationState(visibleDevice.id, calibration)
      } catch (error) {
        await reconcileDirectLanStaleWrite(
          error,
          () => lanRuntime.readCalibration(session),
          (refreshed) => commitCalibrationState(visibleDevice.id, refreshed)
        )
        throw error
      }
      return
    }
    if (visibleDeviceIsLive) {
      const blockedReason = deviceControlBlockReason(visibleDevice)
      if (blockedReason) {
        throw new Error(blockedReason)
      }
    }
    if (visibleDevice.transport === 'devd' && visibleDevice.leaseId && devdBaseUrl) {
      const calibration = await controlClient.configureCalibration(devdBaseUrl, visibleDevice.id, {
        ...request,
        leaseId: visibleDevice.leaseId,
      })
      commitCalibrationState(visibleDevice.id, calibration)
      return
    }

    const calibration = applyLocalCalibrationRequest(visibleCalibration, request)
    commitCalibrationState(visibleDevice.id, calibration)
  }

  const handleCalibrationCapture = async (
    channel: CalibrationChannel,
    options?: { referenceValue?: number; targetAdcMv?: number }
  ) => {
    const request =
      channel === 'rtd_adc'
        ? {
            op: 'capture' as const,
            channel,
            referenceTempC: options?.referenceValue ?? visibleCalibrationRefs.rtdTempC,
            targetAdcMv: options?.targetAdcMv,
            expectedMv: options?.targetAdcMv,
          }
        : {
            op: 'capture' as const,
            channel,
            referenceVinMv: options?.referenceValue ?? visibleCalibrationRefs.vinMv,
          }
    try {
      await updateCalibrationDraft(request)
      setFeedback({
        title: '标定样本已更新',
        detail: `已采集 ${channelLabel(channel)} 样本。`,
        tone: 'success',
      })
      emitEvent('calibration', `captured ${channelLabel(channel)} sample`, 'success')
    } catch (error) {
      setFeedback({
        title: '标定失败',
        detail: errorMessage(error),
        tone: 'warning',
      })
    }
  }

  const handleCalibrationDelete = async (channel: CalibrationChannel, sampleIndex: number) => {
    try {
      await updateCalibrationDraft({ op: 'delete', channel, sampleIndex })
      setFeedback({
        title: '标定样本已更新',
        detail: `已删除 ${channelLabel(channel)} 样本。`,
        tone: 'info',
      })
    } catch (error) {
      setFeedback({
        title: '标定失败',
        detail: errorMessage(error),
        tone: 'warning',
      })
    }
  }

  const handleCalibrationImport = async (calibrationState: CalibrationState) => {
    try {
      await updateCalibrationDraft({ op: 'import', state: calibrationState })
      setFeedback({
        title: '标定数据已导入',
        detail: '共享样本、A/B 槽位与激活槽位已更新。',
        tone: 'success',
      })
    } catch (error) {
      setFeedback({
        title: '标定失败',
        detail: errorMessage(error),
        tone: 'warning',
      })
    }
  }

  const handleCalibrationSetActiveSlot = async (
    channel: CalibrationChannel,
    slot: CalibrationSlotId
  ) => {
    try {
      await updateCalibrationDraft({ op: 'set_active_slot', channel, slot })
      setFeedback({
        title: '激活槽位已切换',
        detail: `${channelLabel(channel)} 已切换到槽位 ${slot.toUpperCase()}。`,
        tone: 'success',
      })
    } catch (error) {
      setFeedback({
        title: '切换槽位失败',
        detail: errorMessage(error),
        tone: 'warning',
      })
    }
  }

  const handleCalibrationSetSlotFit = async (
    channel: CalibrationChannel,
    slot: CalibrationSlotId,
    fit: CalibrationSlotFit
  ) => {
    try {
      await updateCalibrationDraft({ op: 'set_slot_fit', channel, slot, fit })
      setFeedback({
        title: '槽位参数已更新',
        detail: `${channelLabel(channel)} 槽位 ${slot.toUpperCase()} 已写入 ${fit.gain.toFixed(5)}x / ${fit.offsetMv.toFixed(1)}mV。`,
        tone: 'success',
      })
    } catch (error) {
      setFeedback({
        title: '写入槽位失败',
        detail: errorMessage(error),
        tone: 'warning',
      })
    }
  }

  const updateHeaterCurveState = useCallback(
    async (request: Omit<HeaterCurveConfigRequest, 'leaseId'>) => {
      if (isDirectWebSerialDevice(visibleDevice)) {
        if (request.op === 'preview' && request.package) {
          const next = await webSerial.previewHeaterCurve(request.package)
          setHeaterCurveByDevice((current) => ({
            ...current,
            [visibleDevice.id]: next,
          }))
          return next
        }
        if (request.op === 'clear_preview') {
          const next = await webSerial.clearHeaterCurvePreview()
          setHeaterCurveByDevice((current) => ({
            ...current,
            [visibleDevice.id]: next,
          }))
          return next
        }
      }
      if (isDirectLanDevice(visibleDevice)) {
        const session = loadLanDeviceSession(visibleDevice.baseUrl)
        if (!session || !visibleDevice.leaseId) {
          throw new Error('设备未取得有效 LAN lease，请重新选择或配对。')
        }
        try {
          const next = await authorizedLanRequest<HeaterCurveState>(
            session,
            'heater-curve',
            'PUT',
            request,
            visibleDevice.leaseId
          )
          setHeaterCurveByDevice((current) => ({
            ...current,
            [visibleDevice.id]: next,
          }))
          return next
        } catch (error) {
          await reconcileDirectLanStaleWrite(
            error,
            () => lanRuntime.readHeaterCurve(session),
            (refreshed) => {
              setHeaterCurveByDevice((current) => ({
                ...current,
                [visibleDevice.id]: refreshed,
              }))
            }
          )
          throw error
        }
      }
      if (visibleDeviceIsLive) {
        const blockedReason = deviceControlBlockReason(visibleDevice)
        if (blockedReason) {
          throw new Error(blockedReason)
        }
      }
      if (visibleDevice.transport === 'devd' && visibleDevice.leaseId && devdBaseUrl) {
        const next = await controlClient.configureHeaterCurve(devdBaseUrl, visibleDevice.id, {
          ...request,
          leaseId: visibleDevice.leaseId,
        })
        setHeaterCurveByDevice((current) => ({
          ...current,
          [visibleDevice.id]: next,
        }))
        return next
      }

      const next = applyLocalHeaterCurveRequest(visibleHeaterCurve, request)
      setHeaterCurveByDevice((current) => ({
        ...current,
        [visibleDevice.id]: next,
      }))
      return next
    },
    [
      controlClient,
      devdBaseUrl,
      lanRuntime.readHeaterCurve,
      reconcileDirectLanStaleWrite,
      visibleDevice,
      visibleDeviceIsLive,
      visibleHeaterCurve,
      webSerial.clearHeaterCurvePreview,
      webSerial.previewHeaterCurve,
    ]
  )

  const handleHeaterCurvePreview = async (heaterCurve: HeaterCurvePackage) => {
    try {
      await updateHeaterCurveState({ op: 'preview', package: heaterCurve })
      setFeedback({
        title: '加热曲线预览已更新',
        detail: '预览已立即生效；保存后才会写入 EEPROM。',
        tone: 'success',
      })
      emitEvent('calibration', 'updated heater curve preview', 'success')
    } catch (error) {
      setFeedback({
        title: '加热曲线操作失败',
        detail: errorMessage(error),
        tone: 'warning',
      })
    }
  }

  const handleHeaterCurveClearPreview = async () => {
    try {
      await updateHeaterCurveState({ op: 'clear_preview' })
      setFeedback({
        title: '加热曲线预览已清除',
        detail: '预览已移除；当前曲线保持不变。',
        tone: 'info',
      })
      emitEvent('calibration', 'cleared heater curve preview', 'info')
    } catch (error) {
      setFeedback({
        title: '加热曲线操作失败',
        detail: errorMessage(error),
        tone: 'warning',
      })
    }
  }

  const handleHeaterCurveSave = async () => {
    try {
      if (isDirectWebSerialDevice(visibleDevice)) {
        const next = await webSerial.saveHeaterCurve()
        setHeaterCurveByDevice((current) => ({
          ...current,
          [visibleDevice.id]: next,
        }))
      } else if (isDirectLanDevice(visibleDevice)) {
        const session = loadLanDeviceSession(visibleDevice.baseUrl)
        if (!session || !visibleDevice.leaseId) {
          throw new Error('设备未取得有效 LAN lease，请重新选择或配对。')
        }
        try {
          const next = await authorizedLanRequest<HeaterCurveState>(
            session,
            'heater-curve/save',
            'POST',
            undefined,
            visibleDevice.leaseId
          )
          setHeaterCurveByDevice((current) => ({
            ...current,
            [visibleDevice.id]: next,
          }))
        } catch (error) {
          await reconcileDirectLanStaleWrite(
            error,
            () => lanRuntime.readHeaterCurve(session),
            (refreshed) => {
              setHeaterCurveByDevice((current) => ({
                ...current,
                [visibleDevice.id]: refreshed,
              }))
            }
          )
          throw error
        }
      } else if (visibleDevice.transport === 'devd' && visibleDevice.leaseId && devdBaseUrl) {
        const next = await controlClient.saveHeaterCurve(devdBaseUrl, visibleDevice.id, {
          leaseId: visibleDevice.leaseId,
        })
        setHeaterCurveByDevice((current) => ({
          ...current,
          [visibleDevice.id]: next,
        }))
      } else {
        setHeaterCurveByDevice((current) => ({
          ...current,
          [visibleDevice.id]: applyLocalHeaterCurveSave(visibleHeaterCurve),
        }))
      }
      setFeedback({
        title: '加热曲线已保存',
        detail: '预览曲线已写入当前曲线。',
        tone: 'success',
      })
      emitEvent('calibration', 'saved heater curve', 'success')
    } catch (error) {
      setFeedback({
        title: '加热曲线操作失败',
        detail: errorMessage(error),
        tone: 'warning',
      })
    }
  }

  const updateCalibrationRuntime = useCallback(
    async (request: CalibrationControlRequest, failureMessage: string) => {
      const liveUpdated = await configureLiveRuntime({ calibration: request }, failureMessage)
      return liveUpdated
    },
    [configureLiveRuntime]
  )

  const updateCalibrationJob = useCallback(
    async (
      request: {
        op: 'start' | 'cancel'
        kind?: 'vin_adc_auto' | 'thermal_plant_auto'
      },
      failureMessage: string
    ) => {
      const blockedReason = deviceControlBlockReason(visibleDevice)
      if (blockedReason) {
        setFeedback({
          title: '自动校准被阻止',
          detail: blockedReason,
          tone: 'warning',
        })
        emitEvent('devd', 'calibration auto command blocked by transport state', 'warning')
        return false
      }

      try {
        if (isDirectWebSerialDevice(visibleDevice)) {
          await webSerial.configureCalibrationJob(request)
          return true
        }
        if (isDirectLanDevice(visibleDevice)) {
          const session = loadLanDeviceSession(visibleDevice.baseUrl)
          if (!session || !visibleDevice.leaseId) {
            throw new Error('设备未取得有效 LAN lease，请重新选择或配对。')
          }
          try {
            await authorizedLanRequest(
              session,
              'calibration/job',
              'POST',
              request,
              visibleDevice.leaseId
            )
          } catch (error) {
            await reconcileDirectLanStaleWrite(
              error,
              () => lanRuntime.readStatus(session),
              (refreshed) => {
                setPendingDevices((current) =>
                  current.map((device) =>
                    device.id === visibleDevice.id ? applyLanStatus(device, refreshed) : device
                  )
                )
              }
            )
            throw error
          }
          return true
        }
        if (visibleDevice.transport === 'devd' && visibleDevice.leaseId && devdBaseUrl) {
          await controlClient.configureCalibrationJob(devdBaseUrl, visibleDevice.id, {
            leaseId: visibleDevice.leaseId,
            ...request,
          })
          return true
        }
      } catch (error) {
        setFeedback({
          title: '自动校准失败',
          detail: error instanceof Error ? error.message : failureMessage,
          tone: 'warning',
        })
        emitEvent('calibration', failureMessage, 'warning')
      }

      return false
    },
    [
      controlClient,
      devdBaseUrl,
      emitEvent,
      lanRuntime.readStatus,
      reconcileDirectLanStaleWrite,
      visibleDevice,
      webSerial,
    ]
  )

  const handleCalibrationModeExit = async (): Promise<boolean> => {
    const liveUpdated = await updateCalibrationRuntime(
      { mode: 'off', ppsEnabled: false, heaterEnabled: false },
      'calibration mode exit was not accepted'
    )
    if (visibleDeviceIsLive && !liveUpdated) {
      return false
    }
    if (!visibleDeviceIsLive) {
      applyLocalCalibrationRuntimePatch({
        calibration: { mode: 'off', ppsEnabled: false, heaterEnabled: false },
      })
    }
    setFeedback({
      title: '标定模式已退出',
      detail: calibrationModeLabel(visibleCalibrationWorkspaceTab),
      tone: 'success',
    })
    emitEvent('calibration', 'exited calibration mode', 'success')
    return true
  }

  const handleCalibrationModeEnter = async (
    mode: CalibrationWorkbenchMode,
    request: CalibrationControlRequest
  ): Promise<void> => {
    const liveUpdated = await updateCalibrationRuntime(
      { ...request, mode },
      `${calibrationModeLabel(mode)} live control was not accepted`
    )
    if (visibleDeviceIsLive && !liveUpdated) {
      return
    }
    if (!visibleDeviceIsLive) {
      applyLocalCalibrationRuntimePatch({
        calibration: { ...request, mode },
      })
    }
    setFeedback({
      title: `${calibrationModeLabel(mode)}已就绪`,
      detail: calibrationModeLabel(mode),
      tone: 'success',
    })
    emitEvent('calibration', `entered ${calibrationModeLabel(mode)}`, 'success')
  }

  const unavailableRouteState = navigation?.state.kind === 'device' ? navigation.state : null
  if (navigation && unavailableRouteState && (!routeDeviceChoice || routeResumeFailed)) {
    const routeNavigation = navigation
    const recoveryLeaveGuard = calibrationLeaveGuard
      ? {
          nextLabel: calibrationLeaveGuard.nextLabel,
          onDismiss: cancelCalibrationLeaveGuard,
          onContinue: async () => {
            const continueAction = calibrationLeaveGuard.continueAction
            const exited = await handleCalibrationModeExit()
            if (!exited) return
            dismissCalibrationLeaveGuard()
            await continueAction()
          },
        }
      : null
    return (
      <RouteDeviceRecovery
        identityId={unavailableRouteState.deviceId}
        choices={routeDeviceChoices}
        retry={() => window.location.reload()}
        chooseDevice={(identityId) =>
          routeNavigation.navigate({ ...unavailableRouteState, deviceId: identityId })
        }
        addDevice={() => routeNavigation.navigate({ kind: 'add-device' })}
        feedback={feedback}
        leaveGuard={recoveryLeaveGuard}
      />
    )
  }

  if (!visibleDevice) {
    return null
  }

  return (
    <main className="industrial-shell industrial-shell--fixed text-[var(--industrial-text)]">
      <div className="industrial-noise" aria-hidden="true" />
      <div className="industrial-console-wrap">
        <section className="industrial-console">
          <header className="industrial-console__top">
            <div className="industrial-console__identity">
              <div className="industrial-app-mark">
                <span className="industrial-led industrial-led--green" aria-hidden="true" />
                <strong>Flux Purr Link</strong>
                <StatusPill severity={visibleDevice.severity} />
              </div>
              <h1>热控工作台</h1>
            </div>

            <DeviceToolbar
              devices={deviceOptions}
              device={visibleDevice}
              onDeviceChange={handleDeviceChange}
            />
          </header>

          <nav className="industrial-view-tabs" aria-label="Console views">
            {consoleViews.map((view) => {
              const Icon = view.icon
              const isActive = view.id === activeView
              const content = (
                <>
                  <Icon size={18} aria-hidden="true" />
                  <span>
                    <strong>{view.label}</strong>
                    <small>{view.caption}</small>
                  </span>
                </>
              )
              const className = isActive ? 'industrial-view-tab is-selected' : 'industrial-view-tab'
              if (navigation) {
                return (
                  <ConsoleViewLink
                    key={view.id}
                    deviceId={
                      navigation.state.kind === 'device'
                        ? navigation.state.deviceId
                        : deviceIdentityId(visibleDevice)
                    }
                    view={view.id}
                    calibrationTab={visibleCalibrationWorkspaceTab}
                    active={isActive}
                    className={className}
                    search={navigation.search}
                  >
                    {content}
                  </ConsoleViewLink>
                )
              }
              return (
                <button
                  key={view.id}
                  type="button"
                  className={className}
                  aria-pressed={isActive}
                  onClick={() => handleGuardedViewChange(view.id)}
                >
                  {content}
                </button>
              )
            })}
          </nav>

          <div
            className={
              isDeviceAddFlowActive
                ? 'industrial-console__workspace industrial-console__workspace--selection'
                : 'industrial-console__workspace'
            }
          >
            <section className="industrial-panel industrial-console__main">
              <ViewPanel
                view={activeView}
                navigation={navigation}
                device={visibleDevice}
                showDeviceSelection={showDeviceSelection}
                knownDevices={knownDevices}
                allowDemoControls={allowDemoControls}
                webSerial={webSerial}
                selectedPresetIndex={selectedPresetIndex}
                presetTemps={visiblePresetTemps}
                presetEnabled={visiblePresetEnabled}
                fanPolicyValue={visibleFanPolicy}
                flashPhases={visibleFlashPhases}
                artifacts={activeScenario.artifacts}
                artifact={selectedArtifact}
                feedback={feedback}
                calibration={visibleCalibration}
                heaterCurve={visibleHeaterCurve}
                runtimeCalibration={visibleRuntimeCalibration}
                calibrationRefs={visibleCalibrationRefs}
                calibrationWorkspaceTab={visibleCalibrationWorkspaceTab}
                flashRun={flashRun}
                onTargetTempChange={handleTargetTempChange}
                onPresetSlotChange={handlePresetSlotChange}
                onPresetTempChange={handlePresetTempChange}
                onPresetEnabledChange={handlePresetEnabledChange}
                onFanPolicyChange={handleFanPolicyChange}
                onWifiSave={handleWifiSave}
                onWifiClear={handleWifiClear}
                onManualPpsApply={handleManualPpsApply}
                onManualPpsClear={handleManualPpsClear}
                onHeaterHoldToggle={handleHeaterHoldToggle}
                onFaultAttentionAcknowledge={handleFaultAttentionAcknowledge}
                onArtifactChange={handleArtifactChange}
                onCalibrationReferenceChange={setCalibrationReference}
                onCalibrationCapture={handleCalibrationCapture}
                onCalibrationDelete={handleCalibrationDelete}
                onCalibrationImport={handleCalibrationImport}
                onCalibrationSetActiveSlot={handleCalibrationSetActiveSlot}
                onCalibrationSetSlotFit={handleCalibrationSetSlotFit}
                onCalibrationModeEnter={handleCalibrationModeEnter}
                onCalibrationModeExit={handleCalibrationModeExit}
                onCalibrationRuntimeChange={(request, failureMessage) =>
                  void updateCalibrationRuntime(request, failureMessage)
                }
                onCalibrationJobChange={(request, failureMessage) =>
                  void updateCalibrationJob(request, failureMessage)
                }
                onHeaterCurvePreview={handleHeaterCurvePreview}
                onHeaterCurveClearPreview={handleHeaterCurveClearPreview}
                onHeaterCurveSave={handleHeaterCurveSave}
                onCalibrationWorkspaceTabChange={handleGuardedWorkspaceTabChange}
                calibrationLeaveGuard={activeView === 'calibration' ? calibrationLeaveGuard : null}
                onCalibrationLeaveGuardDismiss={cancelCalibrationLeaveGuard}
                onCalibrationLeaveGuardClear={dismissCalibrationLeaveGuard}
                onDeviceSelect={handleDeviceChange}
                onBridgeTargetSelect={handleBridgeTargetSelect}
                controlClient={controlClient}
                devdBaseUrl={devdBaseUrl}
                onQuickAddDevice={handleQuickAddDevice}
                onAddDevice={handleAddDeviceChoice}
                selectedAddDeviceKind={selectedAddDeviceKind}
                onLanPaired={handleLanPaired}
                lanPairing={lanPairing}
                onStartDryRun={handleStartDryRun}
                onStartFlash={handleStartFlash}
              />
            </section>

            {isDeviceAddFlowActive ? null : <GlobalLogPanel events={visibleEvents} />}
          </div>
        </section>
      </div>
    </main>
  )
}

function RouteDeviceRecovery({
  identityId,
  choices,
  retry,
  chooseDevice,
  addDevice,
  feedback,
  leaveGuard,
}: {
  identityId: string
  choices: DeviceChoice[]
  retry: () => void
  chooseDevice: (identityId: string) => void | Promise<void>
  addDevice: () => void | Promise<void>
  feedback: ActionFeedback
  leaveGuard: {
    nextLabel: string
    onDismiss: () => void
    onContinue: () => void
  } | null
}) {
  return (
    <main className="industrial-shell industrial-shell--fixed text-[var(--industrial-text)]">
      <div className="industrial-noise" aria-hidden="true" />
      <section className="industrial-route-message" aria-labelledby="route-device-title">
        <Cable aria-hidden="true" />
        <div>
          <h1 id="route-device-title">目标设备暂不可用</h1>
          <p>
            无法恢复身份 <strong>{identityId}</strong>{' '}
            的已知连接。地址已保留，可重试发现或明确选择其他目标。
          </p>
        </div>
        <div className="industrial-route-message__actions">
          {leaveGuard ? (
            <span id="route-recovery-calibration-anchor">
              <Button type="button" variant="outline">
                <AlertTriangle aria-hidden="true" />
                处理校准退出
              </Button>
              <CalibrationLeaveGuardBubble
                anchorId="route-recovery-calibration-anchor"
                nextLabel={leaveGuard.nextLabel}
                onDismiss={leaveGuard.onDismiss}
                onContinue={leaveGuard.onContinue}
              />
            </span>
          ) : null}
          <Button type="button" variant="outline" onClick={retry}>
            <RefreshCw aria-hidden="true" />
            重试发现
          </Button>
          {choices.map((choice) => (
            <Button
              key={choice.identityId}
              type="button"
              variant="outline"
              onClick={() => chooseDevice(choice.identityId)}
            >
              <Router aria-hidden="true" />
              选择 {choice.name}
            </Button>
          ))}
          <Button type="button" onClick={() => addDevice()}>
            <Plus aria-hidden="true" />
            添加连接
          </Button>
        </div>
        {feedback.tone === 'warning' ? <ActionFeedbackPanel feedback={feedback} /> : null}
      </section>
    </main>
  )
}

function createDemoEventFeed(events: EventLogEntry[], tick: number) {
  if (events.length === 0) {
    return []
  }

  return Array.from({ length: LOG_FEED_SIZE }, (_, index) => {
    const template = events[(index + tick) % events.length]
    const cycle = Math.floor((index + tick) / events.length)
    const totalSeconds = LOG_FEED_START_SECONDS + (index + tick) * LOG_FEED_STEP_SECONDS

    return {
      ...template,
      time: formatLogTime(totalSeconds),
      message:
        cycle > 0
          ? `${template.message} · 第 ${String(index + 1).padStart(4, '0')} 帧`
          : template.message,
    }
  })
}

function formatLogTime(totalSeconds: number) {
  const hours = Math.floor(totalSeconds / 3600) % 24
  const minutes = Math.floor(totalSeconds / 60) % 60
  const seconds = totalSeconds % 60

  return [hours, minutes, seconds].map((value) => String(value).padStart(2, '0')).join(':')
}

export function formatRuntimeEventTime(date: Date) {
  return [date.getHours(), date.getMinutes(), date.getSeconds()]
    .map((value) => String(value).padStart(2, '0'))
    .join(':')
}

function createFlashPhases(
  basePhases: WorkflowPhase[],
  artifact: FirmwareArtifact | undefined,
  device: DeviceTarget,
  flashRun: { status: FlashRunStatus; progress: number }
) {
  const missingFlashCapability = !device.capabilities.includes('flash')
  const leaseBlocked = device.leaseState === 'conflict' || device.leaseState === 'expired'
  const blocked =
    !artifact ||
    artifact.compatibility === 'blocked' ||
    device.severity === 'offline' ||
    missingFlashCapability ||
    leaseBlocked
  const warning = artifact?.compatibility === 'warning' || device.severity === 'warning'

  if (blocked) {
    return basePhases.map((phase, index) => ({
      ...phase,
      state:
        index < 2 ? ('done' as const) : index === 2 ? ('blocked' as const) : ('pending' as const),
      detail:
        index === 2
          ? device.severity === 'offline'
            ? 'Target is offline.'
            : leaseBlocked
              ? 'USB lease is not available for this target.'
              : missingFlashCapability
                ? 'Active transport does not expose flash capability.'
                : 'Selected artifact does not match this device.'
          : phase.detail,
    }))
  }

  if (flashRun.status === 'passed') {
    return basePhases.map((phase) => ({ ...phase, state: 'done' as const }))
  }

  if (flashRun.status === 'running') {
    return basePhases.map((phase, index) => ({
      ...phase,
      state: dryRunPhaseState(index, flashRun.progress),
    }))
  }

  return basePhases.map((phase, index) => ({
    ...phase,
    state:
      index < 2
        ? ('done' as const)
        : index === 2
          ? warning
            ? ('active' as const)
            : ('pending' as const)
          : ('pending' as const),
  }))
}

function dryRunPhaseState(index: number, progress: number): WorkflowPhase['state'] {
  if (index === 0) {
    return 'done'
  }

  if (progress < 35) {
    return index === 1 ? 'active' : 'pending'
  }

  if (progress < 70) {
    return index < 2 ? 'done' : index === 2 ? 'active' : 'pending'
  }

  return index < 3 ? 'done' : 'active'
}

export function createDefaultCalibrationState(): CalibrationState {
  return {
    rtdAdc: createDefaultCalibrationChannelState(),
    vinAdc: createDefaultCalibrationChannelState(),
  }
}

function createDefaultCalibrationSlotFit(): CalibrationSlotFit {
  return {
    gain: 1,
    offsetMv: 0,
  }
}

function createDefaultCalibrationChannelState(): CalibrationChannelState {
  const samples = Array.from({ length: 8 }, () => null) as CalibrationChannelState['samples']
  return {
    samples,
    fittedFit: createCalibrationFit(samples),
    slots: {
      a: createDefaultCalibrationSlotFit(),
      b: createDefaultCalibrationSlotFit(),
    },
    activeSlot: 'a',
  }
}

function isRtdCalibrationSample(
  sample: RtdCalibrationSample | VinCalibrationSample
): sample is RtdCalibrationSample {
  return 'referenceTempC' in sample
}

function isVinCalibrationSample(
  sample: RtdCalibrationSample | VinCalibrationSample
): sample is VinCalibrationSample {
  return !isRtdCalibrationSample(sample)
}

function isValidRtdCalibrationSample(
  sample: RtdCalibrationSample | VinCalibrationSample | null
): sample is RtdCalibrationSample {
  return (
    !!sample &&
    isRtdCalibrationSample(sample) &&
    Number.isFinite(sample.targetAdcMv) &&
    Number.isFinite(sample.referenceTempC)
  )
}

function isValidVinCalibrationSample(
  sample: RtdCalibrationSample | VinCalibrationSample | null
): sample is VinCalibrationSample {
  return !!sample && Number.isFinite(sample.observedMv) && Number.isFinite(sample.expectedMv)
}

function isValidCalibrationSample(
  sample: RtdCalibrationSample | VinCalibrationSample | null,
  channel: CalibrationChannel
) {
  return channel === 'rtd_adc'
    ? isValidRtdCalibrationSample(sample)
    : isValidVinCalibrationSample(sample)
}

function isFitCalibrationSample(
  sample: BaseCalibrationSample | null
): sample is BaseCalibrationSample {
  return !!sample && Number.isFinite(sample.observedMv) && Number.isFinite(sample.expectedMv)
}

function formatRtdCalibrationReference(sample: RtdCalibrationSample) {
  if (sample.referenceTempC != null) {
    return `${sample.referenceTempC.toFixed(1)}℃`
  }
  return '—'
}

function formatRtdCalibrationTargetAdc(sample: RtdCalibrationSample) {
  if (sample.targetAdcMv != null) {
    return `${sample.targetAdcMv}mV`
  }
  return '—'
}

function cloneCalibrationSamples(
  samples: CalibrationChannelState['samples']
): CalibrationChannelState['samples'] {
  return samples.map((sample) => (sample ? { ...sample } : null))
}

function cloneCalibrationChannelState(
  channelState: CalibrationChannelState
): CalibrationChannelState {
  return {
    samples: cloneCalibrationSamples(channelState.samples),
    fittedFit: { ...channelState.fittedFit },
    slots: {
      a: { ...channelState.slots.a },
      b: { ...channelState.slots.b },
    },
    activeSlot: channelState.activeSlot,
  }
}

function cloneCalibrationState(state: CalibrationState): CalibrationState {
  return {
    rtdAdc: cloneCalibrationChannelState(state.rtdAdc),
    vinAdc: cloneCalibrationChannelState(state.vinAdc),
  }
}

function applyLocalCalibrationRuntimeRequest(
  current: CalibrationRuntimeState,
  request: CalibrationControlRequest
): CalibrationRuntimeState {
  const nextMode = request.mode ?? current.mode
  const nextPpsEnabled =
    nextMode === 'off' ? false : (request.ppsEnabled ?? current.ppsEnabled ?? false)
  const nextHeaterEnabled =
    nextMode === 'off' ? false : (request.heaterEnabled ?? current.heaterEnabled)
  const nextTargetAdcMv = request.targetAdcMv ?? current.targetAdcMv ?? null

  return {
    ...current,
    mode: nextMode,
    ppsEnabled: nextPpsEnabled,
    ppsMv: nextPpsEnabled ? (request.ppsMv ?? current.ppsMv ?? null) : null,
    ppsMa: nextPpsEnabled ? current.ppsMa : null,
    heaterEnabled: nextHeaterEnabled,
    targetAdcMv: nextTargetAdcMv,
    stable:
      nextMode === 'rtd_adc'
        ? nextPpsEnabled && nextHeaterEnabled && nextTargetAdcMv != null
        : false,
    stabilityErrorMv:
      nextMode === 'rtd_adc' && nextTargetAdcMv != null && nextPpsEnabled && nextHeaterEnabled
        ? 0
        : null,
    error: null,
    job:
      nextMode === 'off'
        ? {
            ...current.job,
            kind: null,
            status: 'idle',
            progressPercent: 0,
            samplesCollected: 0,
            nextRequestMv: null,
            message: null,
          }
        : current.job,
  }
}

function createCalibrationFit(samples: Array<BaseCalibrationSample | null>) {
  const custom = samples.filter(isFitCalibrationSample)
  if (custom.length === 0) {
    return {
      gain: 1,
      offsetMv: 0,
      sampleCount: 0,
    }
  }
  if (custom.length === 1) {
    const sample = custom[0]
    return {
      gain: 1,
      offsetMv: sample.expectedMv - sample.observedMv,
      sampleCount: 1,
    }
  }
  const points = custom
  const n = points.length
  const sumX = points.reduce((sum, sample) => sum + sample.observedMv, 0)
  const sumY = points.reduce((sum, sample) => sum + sample.expectedMv, 0)
  const sumXX = points.reduce((sum, sample) => sum + sample.observedMv * sample.observedMv, 0)
  const sumXY = points.reduce((sum, sample) => sum + sample.observedMv * sample.expectedMv, 0)
  const denominator = n * sumXX - sumX * sumX
  const gain = Math.abs(denominator) < Number.EPSILON ? 1 : (n * sumXY - sumX * sumY) / denominator
  const offsetMv =
    Math.abs(denominator) < Number.EPSILON ? (sumY - sumX) / n : (sumY - gain * sumX) / n
  return {
    gain,
    offsetMv,
    sampleCount: points.length,
  }
}

function calibrationSampleKeys(samples: Array<BaseCalibrationSample | null>) {
  const seen = new Map<string, number>()
  return samples.map((sample) => {
    if (!sample) {
      return null
    }
    const base = `${sample.observedMv}-${sample.expectedMv}`
    const ordinal = seen.get(base) ?? 0
    seen.set(base, ordinal + 1)
    return `${base}-${ordinal}`
  })
}

export function applyLocalCalibrationRequest(
  current: CalibrationState,
  request: Omit<CalibrationConfigRequest, 'leaseId'>
): CalibrationState {
  const next = cloneCalibrationState(current)
  if (request.op === 'import') {
    return request.state ? normalizeCalibrationState(request.state) : next
  }
  const channel = request.channel
  if (!channel) {
    throw new Error('缺少标定通道。')
  }
  const channelState = channel === 'rtd_adc' ? next.rtdAdc : next.vinAdc
  const samples = channelState.samples
  if (request.op === 'clear') {
    samples.fill(null)
  } else if (request.op === 'delete') {
    if (request.sampleIndex == null || !samples[request.sampleIndex]) {
      throw new Error('未找到对应样本。')
    }
    samples[request.sampleIndex] = null
  } else if (request.op === 'capture') {
    const slot = samples.findIndex((sample) => sample == null)
    if (slot < 0) {
      throw new Error('该标定通道已达到 8 个样本上限。')
    }
    const observedMv = request.observedMv ?? (channel === 'rtd_adc' ? 1120 : 1670)
    const expectedMv =
      request.expectedMv ??
      (channel === 'rtd_adc' ? request.targetAdcMv : vinAdcMvForInput(request.referenceVinMv ?? 0))
    if (expectedMv == null) {
      throw new Error(channel === 'rtd_adc' ? '缺少目标 ADC。' : '缺少标定参考。')
    }
    samples[slot] =
      channel === 'rtd_adc'
        ? {
            observedMv,
            expectedMv,
            referenceTempC: request.referenceTempC,
            targetAdcMv: request.targetAdcMv,
          }
        : {
            observedMv,
            expectedMv,
            referenceVinMv: request.referenceVinMv,
          }
  } else if (request.op === 'set_active_slot') {
    if (!request.slot) {
      throw new Error('缺少槽位。')
    }
    channelState.activeSlot = request.slot
  } else if (request.op === 'set_slot_fit') {
    if (!request.slot || !request.fit) {
      throw new Error('缺少槽位拟合参数。')
    }
    channelState.slots[request.slot] = { ...request.fit }
  }
  return normalizeCalibrationState(next)
}

function normalizeCalibrationChannelSamples<TSample extends BaseCalibrationSample>(
  samples: Array<TSample | null>
): Array<TSample | null> {
  const compacted = samples.filter(isFitCalibrationSample) as TSample[]
  return Array.from({ length: 8 }, (_, index) => compacted[index] ?? null)
}

function normalizeCalibrationChannelState(
  channelState: CalibrationChannelState,
  channel: CalibrationChannel
): CalibrationChannelState {
  const samples = normalizeCalibrationChannelSamples(
    channel === 'rtd_adc'
      ? (channelState.samples as Array<RtdCalibrationSample | null>)
      : (channelState.samples as Array<VinCalibrationSample | null>)
  )
  return {
    ...channelState,
    samples,
    fittedFit: createCalibrationFit(samples),
  }
}

function normalizeCalibrationState(calibrationState: CalibrationState): CalibrationState {
  return {
    rtdAdc: normalizeCalibrationChannelState(calibrationState.rtdAdc, 'rtd_adc'),
    vinAdc: normalizeCalibrationChannelState(calibrationState.vinAdc, 'vin_adc'),
  }
}

function vinAdcMvForInput(inputMv: number) {
  return Math.round((inputMv * 5100) / (56_000 + 5100))
}

function channelLabel(channel: CalibrationChannel) {
  return channel === 'rtd_adc' ? '温度 ADC' : '电压 ADC'
}

function calibrationFitMode(fit: CalibrationFit) {
  if (fit.sampleCount >= 2) {
    return '自定义'
  }
  if (fit.sampleCount === 1) {
    return '单点'
  }
  return '默认'
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : '请求失败。'
}

function clearCalibrationLoadWarning(current: ActionFeedback): ActionFeedback {
  if (current.title === 'Calibration unavailable' || current.title === 'Heater curve unavailable') {
    return {
      title: '标定数据已同步',
      detail: '当前标定数据来自 devd 固件状态。',
      tone: 'info',
    }
  }

  return current
}

export function clearStaleWebSerialFailure<T extends ActionFeedback>(
  current: T
): T | ActionFeedback {
  if (current.title !== 'Web Serial unavailable') {
    return current
  }

  return {
    title: 'Web Serial connected',
    detail: 'Browser direct USB JSONL control is active.',
    tone: 'success',
  }
}

function isTransportBlockedFeedback(current: ActionFeedback) {
  return (
    current.title === '设备租约冲突' ||
    current.title === '硬件连接受阻' ||
    current.title === '目标温度更新被阻止'
  )
}

function isNoLiveTargetDevice(device: Pick<DeviceTarget, 'id' | 'transport'>) {
  return device.id === NO_LIVE_TARGET_ID && device.transport === 'serial'
}

export function devicePickerTargets<
  T extends Pick<DeviceTarget, 'id' | 'transport' | 'connectionAvailable'>,
>(devices: T[]) {
  return devices.filter(
    (candidate) => !isNoLiveTargetDevice(candidate) && candidate.connectionAvailable !== false
  )
}

function isKnownDeviceChoice(device: DeviceTarget) {
  return !isNoLiveTargetDevice(device) && !isDirectWebSerialDevice(device)
}

function isPendingDeviceChoice(device: DeviceTarget) {
  return device.id.startsWith('pending-')
}

function isLiveRuntimeDevice(device: Pick<DeviceTarget, 'transport' | 'baseUrl'>) {
  return device.transport === 'devd' || isDirectWebSerialDevice(device) || isDirectLanDevice(device)
}

export function shouldShowDeviceControlBlockFeedback(
  device: Pick<DeviceTarget, 'transport' | 'baseUrl' | 'connectionAvailable'>
) {
  return device.connectionAvailable !== false && isLiveRuntimeDevice(device)
}

function isControlPlaneStatus(value: unknown): value is ControlPlaneStatus {
  if (!value || typeof value !== 'object') {
    return false
  }
  const status = value as Record<string, unknown>
  return (
    typeof status.currentTempC === 'number' &&
    typeof status.targetTempC === 'number' &&
    typeof status.heaterEnabled === 'boolean' &&
    typeof status.heaterOutputPercent === 'number' &&
    typeof status.activeCoolingEnabled === 'boolean' &&
    typeof status.fanDisplayState === 'string' &&
    typeof status.voltageMv === 'number' &&
    typeof status.currentMa === 'number' &&
    typeof status.boardTempCenti === 'number' &&
    typeof status.pdRequestMv === 'number' &&
    typeof status.pdContractMv === 'number' &&
    typeof status.pdState === 'string' &&
    typeof status.calibration === 'object' &&
    status.calibration !== null &&
    typeof status.network === 'object' &&
    status.network !== null
  )
}

function applyLanStatus(device: DeviceTarget, status: ControlPlaneStatus): DeviceTarget {
  const networkState = status.network.state
  return {
    ...device,
    severity: networkState === 'error' || networkState === 'timeout' ? 'warning' : 'nominal',
    uptime: formatDeviceUptime(status.uptimeSeconds),
    boardTempC: status.boardTempCenti / 100,
    currentTempC: status.currentTempC,
    targetTempC: status.targetTempC,
    selectedPresetIndex: status.selectedPresetSlot,
    presetsC: status.presetsC,
    rtdRawAdcMv: status.rtdRawAdcMv,
    vinRawAdcMv: status.vinRawAdcMv,
    voltageMv: status.voltageMv,
    currentMa: status.currentMa,
    pdRequestMv: status.pdRequestMv,
    pdContractMv: status.pdContractMv,
    pdState: status.pdState,
    manualPpsEnabled: status.manualPpsEnabled ?? false,
    manualPpsMv: status.manualPpsMv ?? null,
    manualPpsMa: status.manualPpsMa ?? null,
    ppsCapabilityMinMv: status.ppsCapabilityMinMv ?? null,
    ppsCapabilityMaxMv: status.ppsCapabilityMaxMv ?? null,
    ppsCapabilityMaxMa: status.ppsCapabilityMaxMa ?? null,
    manualPpsError: status.manualPpsError ?? null,
    faultAttentionPending: status.faultAttentionPending ?? false,
    heaterLockReason: status.heaterLockReason ?? null,
    calibration: status.calibration,
    heaterEnabled: status.heaterEnabled,
    heaterOutputPercent: status.heaterOutputPercent,
    activeCoolingEnabled: status.activeCoolingEnabled,
    fanState: status.fanDisplayState,
    wifiSsid: status.network.ssid ?? device.wifiSsid,
    wifiRssi: status.network.wifiRssi ?? null,
    wifiPasswordLength: status.network.wifiPasswordLength ?? device.wifiPasswordLength,
    networkState,
    configurationGeneration:
      status.network.configurationGeneration ?? device.configurationGeneration,
    transitionSequence: status.network.transitionSequence ?? device.transitionSequence,
    wifiFailureCode: status.network.failureCode ?? null,
    transportIssue: device.transportIssue,
  }
}

function formatDeviceUptime(seconds: number) {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const rest = seconds % 60
  return [hours, minutes, rest].map((value) => String(value).padStart(2, '0')).join(':')
}

function clampPresetIndex(value: number | undefined) {
  if (!Number.isFinite(value)) {
    return 3
  }
  return Math.min(PRESET_SLOT_IDS.length - 1, Math.max(0, Math.trunc(value ?? 3)))
}

function normalizePresets(presets: Array<number | null> | undefined) {
  if (!presets || presets.length !== PRESET_SLOT_IDS.length) {
    return PRESETS_C
  }
  return presets.map((preset) => (typeof preset === 'number' ? clampTargetTemp(preset) : null))
}

function presetTempsFromValues(presets: Array<number | null>) {
  return presets.map((preset, index) => preset ?? PRESET_TEMPS_C[index] ?? TARGET_TEMP_MIN)
}

function presetEnabledFromValues(presets: Array<number | null>) {
  return presets.map((preset) => preset != null)
}

function presetValuesFromEditorState(presetTemps: number[], presetEnabled: boolean[]) {
  return PRESET_SLOT_IDS.map((_, index) =>
    presetEnabled[index] ? (presetTemps[index] ?? PRESET_TEMPS_C[index] ?? TARGET_TEMP_MIN) : null
  )
}

function fanPolicyFromDevice(device: DeviceTarget): DeviceTarget['fanState'] {
  return device.activeCoolingEnabled ? 'AUTO' : 'OFF'
}

function formatTemp(value: number) {
  if (!isRenderableTemperature(value)) {
    return 'N/A'
  }

  return `${formatTempNumber(value)}℃`
}

function formatPresetTemp(value: number, enabled: boolean) {
  return enabled ? `${formatTempNumber(value)}℃` : '---'
}

function formatTempNumber(value: number) {
  if (!isRenderableTemperature(value)) {
    return 'N/A'
  }
  return value.toFixed(1).replace(/\.0$/, '')
}

function clampTargetTemp(value: number) {
  return Math.min(TARGET_TEMP_MAX, Math.max(TARGET_TEMP_MIN, Math.round(value)))
}

function ppsCapabilityRange(device: DeviceTarget) {
  const minMv = Math.max(device.ppsCapabilityMinMv ?? 0, PPS_HARDWARE_MIN_MV)
  const maxMv = Math.min(device.ppsCapabilityMaxMv ?? 0, PPS_HARDWARE_MAX_MV)
  if (minMv <= 0 || maxMv < minMv) {
    return null
  }
  return { minMv, maxMv }
}

function clampPpsMv(value: number, device: DeviceTarget) {
  const range = ppsCapabilityRange(device)
  const minMv = range?.minMv ?? PPS_STEP_MV
  const maxMv = range?.maxMv ?? PPS_HARDWARE_MAX_MV
  const rounded = Math.round(value / PPS_STEP_MV) * PPS_STEP_MV
  return Math.min(maxMv, Math.max(minMv, rounded))
}

function defaultManualPpsMv(device: DeviceTarget) {
  return clampPpsMv(
    device.manualPpsMv ?? device.pdContractMv ?? device.ppsCapabilityMinMv ?? 12_000,
    device
  )
}

function effectivePpsCurrentCapabilityMa(device: DeviceTarget) {
  return device.currentMa > 0 ? device.currentMa : (device.ppsCapabilityMaxMa ?? null)
}

function calibrationModeLabel(mode: CalibrationWorkbenchMode) {
  switch (mode) {
    case 'vin_adc':
      return '电压读数标定'
    case 'rtd_adc':
      return '温度标定'
    case 'heater_curve':
      return '加热曲线标定'
  }
}

function consoleViewLabel(view: ConsoleView) {
  switch (view) {
    case 'dashboard':
      return '总览'
    case 'settings':
      return '设置'
    case 'calibration':
      return '校准'
    case 'update':
      return '更新'
    case 'add-device':
      return '添加设备'
  }
}

function asWorkbenchMode(mode: CalibrationMode): CalibrationWorkbenchMode | null {
  if (mode === 'thermal_plant') {
    return 'heater_curve'
  }
  if (mode === 'vin_adc' || mode === 'rtd_adc' || mode === 'heater_curve') {
    return mode
  }
  return null
}

function calibrationPpsDraft(device: DeviceTarget, calibration: CalibrationRuntimeState) {
  return {
    millivolts: calibration.ppsMv ?? device.manualPpsMv ?? defaultManualPpsMv(device),
  }
}

function validateCalibrationPpsInput(device: DeviceTarget, millivolts: number) {
  const boundedMv = clampPpsMv(millivolts, device)
  if (boundedMv !== millivolts) {
    return 'PPS 请求必须在实时 capability 内，并满足 100mV 步进。'
  }
  return null
}

function calibrationPowerCapability(device: DeviceTarget) {
  const range = ppsCapabilityRange(device)
  const currentProxyMa = effectivePpsCurrentCapabilityMa(device)
  const warnings: string[] = []

  if (!range) {
    warnings.push('当前电源没有可用的 PPS 能力。')
  }

  if (device.transportIssue) {
    warnings.push(device.transportIssue)
  }

  const summary = range
    ? `PPS ${formatVolts(range.minMv)} - ${formatVolts(range.maxMv)}`
    : 'PPS 能力不可用'

  return {
    summary,
    currentProxyMa,
    warnings,
    ok: warnings.length === 0,
  }
}

function formatVolts(millivolts: number) {
  if (millivolts <= 0) {
    return 'N/A'
  }

  return `${(millivolts / 1000).toFixed(millivolts % 1000 === 0 ? 0 : 1)}V`
}

function formatAmps(milliamps: number) {
  if (milliamps <= 0) {
    return 'N/A'
  }
  return `${(milliamps / 1000).toFixed(2)}A`
}

function formatPdCapability(milliamps: number) {
  const amps = formatAmps(milliamps)
  return amps === 'N/A' ? '能力未知' : `电流能力 ${amps}`
}

function pdStateLabel(state: DeviceTarget['pdState']) {
  const labels: Record<DeviceTarget['pdState'], string> = {
    negotiating: '协商中',
    ready: '已就绪',
    fallback_5v: '回落',
    fault: '故障',
  }

  return labels[state]
}

function temperatureBand(tempC: number) {
  if (!isRenderableTemperature(tempC)) {
    return 'cool'
  }
  if (tempC >= 300) {
    return 'overtemp'
  }
  if (tempC >= 250) {
    return 'hot'
  }
  if (tempC >= 180) {
    return 'active'
  }
  if (tempC >= 60) {
    return 'warm'
  }

  return 'cool'
}

export function DeviceToolbar({
  devices,
  device,
  onDeviceChange,
}: {
  devices: DeviceTarget[]
  device: DeviceTarget
  onDeviceChange: (deviceId: string) => void
}) {
  return (
    <section className="industrial-status-strip" aria-label="当前目标">
      <div className="industrial-target-picker">
        <DeviceTargetPicker devices={devices} device={device} onDeviceChange={onDeviceChange} />
      </div>

      <StatusDatum label="传输" value={transportLabels[device.transport]} />
      <StatusDatum
        label="租约"
        value={device.leaseState ? leaseStateLabels[device.leaseState] : '无'}
      />
      <StatusDatum label="热板" value={formatTemp(device.currentTempC)} />
      <StatusDatum label="PD" value={formatVolts(device.pdContractMv)} />
    </section>
  )
}

export function DeviceTargetPicker({
  devices,
  device,
  onDeviceChange,
}: {
  devices: DeviceTarget[]
  device: DeviceTarget
  onDeviceChange: (deviceId: string) => void
}) {
  const [open, setOpen] = useState(false)
  const pickerRef = useRef<HTMLDivElement>(null)
  const choices = useMemo(() => mergeDeviceChoices(devicePickerTargets(devices)), [devices])

  useEffect(() => {
    if (!open) return
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!pickerRef.current?.contains(event.target as Node)) setOpen(false)
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }
    document.addEventListener('pointerdown', closeOnOutsidePointer)
    document.addEventListener('keydown', closeOnEscape)
    return () => {
      document.removeEventListener('pointerdown', closeOnOutsidePointer)
      document.removeEventListener('keydown', closeOnEscape)
    }
  }, [open])

  const chooseConnection = (targetId: string) => {
    setOpen(false)
    onDeviceChange(targetId)
  }

  return (
    <div ref={pickerRef} className="industrial-device-picker">
      <button
        type="button"
        className="industrial-device-select industrial-device-picker__trigger"
        aria-label="目标设备"
        aria-expanded={open}
        aria-haspopup="dialog"
        onClick={() => setOpen((current) => !current)}
      >
        <span className="industrial-device-select-value">
          <strong>{device.alias}</strong>
          <small>
            {transportLabels[device.transport]} · {device.location}
          </small>
        </span>
        <ChevronDown aria-hidden="true" className={open ? 'rotate-180' : undefined} />
      </button>
      {open ? (
        <div
          className="industrial-device-picker__popover"
          role="dialog"
          aria-label="设备与连接方式"
        >
          {choices.map((choice) => (
            <DeviceChoiceCard
              key={choice.identityId}
              choice={choice}
              activeTargetId={device.id}
              onChoose={chooseConnection}
            />
          ))}
          <button
            type="button"
            className="industrial-device-picker__add"
            onClick={() => chooseConnection(ADD_DEVICE_VALUE)}
          >
            <Plus aria-hidden="true" />
            添加设备
          </button>
        </div>
      ) : null}
    </div>
  )
}

function DeviceChoiceCard({
  choice,
  activeTargetId,
  onChoose,
}: {
  choice: DeviceChoice
  activeTargetId?: string
  onChoose: (targetId: string) => void
}) {
  return (
    <article className="industrial-device-choice-card" data-device-id={choice.identityId}>
      <header className="industrial-device-choice-card__header">
        <span>
          <strong>{choice.name}</strong>
          <small>设备 ID · {choice.identityId}</small>
        </span>
        <em>{severityLabels[choice.primary.severity]}</em>
      </header>
      <fieldset className="industrial-device-choice-card__connections">
        <legend className="sr-only">{choice.name} 连接方式</legend>
        {choice.connections.map((connection) => (
          <DeviceConnectionButton
            key={connection.key}
            connection={connection}
            active={connection.target.id === activeTargetId}
            onChoose={onChoose}
          />
        ))}
      </fieldset>
    </article>
  )
}

function DeviceConnectionButton({
  connection,
  active,
  onChoose,
}: {
  connection: DeviceConnectionOption
  active: boolean
  onChoose: (targetId: string) => void
}) {
  const ConnectionIcon =
    connection.kind === 'wifi'
      ? Wifi
      : connection.kind === 'web-serial'
        ? Cable
        : connection.kind === 'bridge'
          ? Router
          : CircleHelp

  return (
    <button
      type="button"
      className={cn('industrial-device-connection-button', active && 'is-active')}
      onClick={() => onChoose(connection.target.id)}
      aria-label={`${connection.label} · ${connection.detail} · ${connection.target.alias}`}
      aria-pressed={active}
    >
      <ConnectionIcon aria-hidden="true" className="industrial-device-connection-button__icon" />
      <span>
        <strong>{connection.label}</strong>
        <small>{connection.detail}</small>
      </span>
      <ChevronDown aria-hidden="true" className="industrial-device-connection-button__arrow" />
    </button>
  )
}

function ViewPanel({
  view,
  navigation,
  device,
  showDeviceSelection,
  knownDevices,
  allowDemoControls,
  webSerial,
  selectedPresetIndex,
  presetTemps,
  presetEnabled,
  fanPolicyValue,
  flashPhases,
  artifacts,
  artifact,
  feedback,
  calibration,
  heaterCurve,
  runtimeCalibration,
  calibrationRefs,
  calibrationWorkspaceTab,
  flashRun,
  onTargetTempChange,
  onPresetSlotChange,
  onPresetTempChange,
  onPresetEnabledChange,
  onFanPolicyChange,
  onWifiSave,
  onWifiClear,
  onManualPpsApply,
  onManualPpsClear,
  onHeaterHoldToggle,
  onFaultAttentionAcknowledge,
  onArtifactChange,
  onDeviceSelect,
  onBridgeTargetSelect,
  controlClient,
  devdBaseUrl,
  onQuickAddDevice,
  onAddDevice,
  selectedAddDeviceKind,
  onLanPaired,
  lanPairing,
  onStartDryRun,
  onStartFlash,
  onCalibrationReferenceChange,
  onCalibrationCapture,
  onCalibrationDelete,
  onCalibrationImport,
  onCalibrationSetActiveSlot,
  onCalibrationSetSlotFit,
  onCalibrationModeEnter,
  onCalibrationModeExit,
  onCalibrationRuntimeChange,
  onCalibrationJobChange,
  onHeaterCurvePreview,
  onHeaterCurveClearPreview,
  onHeaterCurveSave,
  onCalibrationWorkspaceTabChange,
  calibrationLeaveGuard,
  onCalibrationLeaveGuardDismiss,
  onCalibrationLeaveGuardClear,
}: {
  view: ConsoleView
  navigation?: ConsoleNavigationAdapter
  device: DeviceTarget
  showDeviceSelection: boolean
  knownDevices: DeviceTarget[]
  allowDemoControls: boolean
  webSerial: Pick<LiveWebSerialControls, 'state' | 'supported'>
  selectedPresetIndex: number
  presetTemps: number[]
  presetEnabled: boolean[]
  fanPolicyValue: DeviceTarget['fanState']
  flashPhases: WorkflowPhase[]
  artifacts: FirmwareArtifact[]
  artifact?: FirmwareArtifact
  feedback: ActionFeedback
  calibration: CalibrationState
  heaterCurve: HeaterCurveState
  runtimeCalibration: CalibrationRuntimeState
  calibrationRefs: { rtdTempC: number; vinMv: number }
  calibrationWorkspaceTab: CalibrationWorkspaceTab
  flashRun: { status: FlashRunStatus; progress: number }
  onTargetTempChange: (nextTargetTemp: number) => void
  onPresetSlotChange: (presetIndex: number) => void | Promise<void>
  onPresetTempChange: (nextTempC: number) => void | Promise<void>
  onPresetEnabledChange: (nextEnabled: boolean) => void | Promise<void>
  onFanPolicyChange: (fanState: DeviceTarget['fanState']) => void
  onWifiSave: (draft: WifiNetworkSettingsDraft) => Promise<NetworkSummary>
  onWifiClear: () => Promise<NetworkSummary>
  onManualPpsApply: (millivolts: number) => void | Promise<void>
  onManualPpsClear: () => void | Promise<void>
  onHeaterHoldToggle: () => void
  onFaultAttentionAcknowledge: () => void | Promise<void>
  onArtifactChange: (artifactId: string) => void
  onDeviceSelect: (deviceId: string) => void
  onBridgeTargetSelect: (device: DeviceTarget) => void
  controlClient: ControlPlaneHttpClient
  devdBaseUrl: string | null
  onQuickAddDevice: (kind: AddDeviceKind) => void
  onAddDevice: (kind: AddDeviceKind) => void
  selectedAddDeviceKind: AddDeviceKind
  onLanPaired: (session: LanDeviceSession, probe: LanProbe) => void | Promise<void>
  lanPairing?: LanPairingOverrides
  onStartDryRun: () => void
  onStartFlash: () => void
  onCalibrationReferenceChange: (channel: CalibrationChannel, value: number) => void
  onCalibrationCapture: (
    channel: CalibrationChannel,
    options?: { referenceValue?: number; targetAdcMv?: number }
  ) => void | Promise<void>
  onCalibrationDelete: (channel: CalibrationChannel, sampleIndex: number) => void | Promise<void>
  onCalibrationImport: (calibrationState: CalibrationState) => void | Promise<void>
  onCalibrationSetActiveSlot: (
    channel: CalibrationChannel,
    slot: CalibrationSlotId
  ) => void | Promise<void>
  onCalibrationSetSlotFit: (
    channel: CalibrationChannel,
    slot: CalibrationSlotId,
    fit: CalibrationSlotFit
  ) => void | Promise<void>
  onCalibrationModeEnter: (
    mode: CalibrationWorkbenchMode,
    request: CalibrationControlRequest
  ) => void | Promise<void>
  onCalibrationModeExit: () => boolean | Promise<boolean>
  onCalibrationRuntimeChange: (
    request: Partial<CalibrationControlRequest>,
    failureMessage: string
  ) => void | Promise<void>
  onCalibrationJobChange: (
    request: {
      op: 'start' | 'cancel'
      kind?: 'vin_adc_auto' | 'thermal_plant_auto'
    },
    failureMessage: string
  ) => void | Promise<void>
  onHeaterCurvePreview: (heaterCurve: HeaterCurvePackage) => void | Promise<void>
  onHeaterCurveClearPreview: () => void | Promise<void>
  onHeaterCurveSave: () => void | Promise<void>
  onCalibrationWorkspaceTabChange: (nextTab: CalibrationWorkspaceTab) => void
  calibrationLeaveGuard: CalibrationLeaveGuardState | null
  onCalibrationLeaveGuardDismiss: () => void
  onCalibrationLeaveGuardClear: () => void
}) {
  if (showDeviceSelection) {
    return (
      <DeviceSelectionView
        knownDevices={knownDevices}
        allowDemoControls={allowDemoControls}
        webSerial={webSerial}
        feedback={feedback}
        onDeviceSelect={onDeviceSelect}
        onAddDevice={onQuickAddDevice}
      />
    )
  }

  if (view === 'add-device') {
    return (
      <AddDeviceView
        allowDemoControls={allowDemoControls}
        webSerial={webSerial}
        feedback={feedback}
        onAddDevice={onAddDevice}
        selectedAddDeviceKind={selectedAddDeviceKind}
        onLanPaired={onLanPaired}
        lanPairing={lanPairing}
        knownDevices={knownDevices}
        onBridgeTargetSelect={onBridgeTargetSelect}
        controlClient={controlClient}
        devdBaseUrl={devdBaseUrl}
      />
    )
  }

  if (view === 'settings') {
    return (
      <SettingsView
        device={device}
        fanPolicyValue={fanPolicyValue}
        selectedPresetIndex={selectedPresetIndex}
        presetTemps={presetTemps}
        presetEnabled={presetEnabled}
        feedback={feedback}
        onPresetSlotChange={onPresetSlotChange}
        onPresetTempChange={onPresetTempChange}
        onPresetEnabledChange={onPresetEnabledChange}
        onFanPolicyChange={onFanPolicyChange}
        onWifiSave={onWifiSave}
        onWifiClear={onWifiClear}
      />
    )
  }

  if (view === 'update') {
    return (
      <UpdateView
        device={device}
        artifacts={artifacts}
        artifact={artifact}
        flashPhases={flashPhases}
        feedback={feedback}
        flashRun={flashRun}
        onArtifactChange={onArtifactChange}
        onStartDryRun={onStartDryRun}
        onStartFlash={onStartFlash}
      />
    )
  }

  if (view === 'calibration') {
    return (
      <CalibrationView
        navigation={navigation}
        device={device}
        calibration={calibration}
        heaterCurve={heaterCurve}
        runtimeCalibration={runtimeCalibration}
        refs={calibrationRefs}
        feedback={feedback}
        calibrationWorkspaceTab={calibrationWorkspaceTab}
        onTargetTempChange={onTargetTempChange}
        onReferenceChange={onCalibrationReferenceChange}
        onCapture={onCalibrationCapture}
        onDelete={onCalibrationDelete}
        onImport={onCalibrationImport}
        onCalibrationSetActiveSlot={onCalibrationSetActiveSlot}
        onCalibrationSetSlotFit={onCalibrationSetSlotFit}
        onModeEnter={onCalibrationModeEnter}
        onModeExit={onCalibrationModeExit}
        onCalibrationRuntimeChange={onCalibrationRuntimeChange}
        onCalibrationJobChange={onCalibrationJobChange}
        onHeaterCurvePreview={onHeaterCurvePreview}
        onHeaterCurveClearPreview={onHeaterCurveClearPreview}
        onHeaterCurveSave={onHeaterCurveSave}
        onCalibrationWorkspaceTabChange={onCalibrationWorkspaceTabChange}
        calibrationLeaveGuard={calibrationLeaveGuard}
        onCalibrationLeaveGuardDismiss={onCalibrationLeaveGuardDismiss}
        onCalibrationLeaveGuardClear={onCalibrationLeaveGuardClear}
      />
    )
  }

  return (
    <DashboardView
      device={device}
      artifact={artifact}
      feedback={feedback}
      onTargetTempChange={onTargetTempChange}
      onManualPpsApply={onManualPpsApply}
      onManualPpsClear={onManualPpsClear}
      onHeaterHoldToggle={onHeaterHoldToggle}
      onFaultAttentionAcknowledge={onFaultAttentionAcknowledge}
    />
  )
}

function DeviceSelectionView({
  knownDevices,
  allowDemoControls,
  webSerial,
  feedback,
  onDeviceSelect,
  onAddDevice,
}: {
  knownDevices: DeviceTarget[]
  allowDemoControls: boolean
  webSerial: Pick<LiveWebSerialControls, 'state' | 'supported'>
  feedback: ActionFeedback
  onDeviceSelect: (deviceId: string) => void
  onAddDevice: (kind: AddDeviceKind) => void
}) {
  const choices = useMemo(
    () => mergeDeviceChoices(knownDevices, { allowDemoControls }),
    [allowDemoControls, knownDevices]
  )

  return (
    <div className="industrial-view-panel industrial-device-select-view">
      <PanelHeader kicker="Device" title="Choose target" />
      <section className="industrial-device-select-section" aria-label="Known devices">
        {choices.length > 0 ? (
          <div className="industrial-known-device-grid">
            {choices.map((choice) => (
              <DeviceChoiceCard key={choice.identityId} choice={choice} onChoose={onDeviceSelect} />
            ))}
          </div>
        ) : (
          <div className="industrial-empty-device-grid">
            <strong>No known devices</strong>
            <span>Connect a new target from one of the options below.</span>
          </div>
        )}
      </section>

      <hr className="industrial-device-select-divider" />

      <section className="industrial-device-select-section" aria-label="Add device">
        <AddDeviceChoices
          allowDemoControls={allowDemoControls}
          webSerial={webSerial}
          onAddDevice={onAddDevice}
        />
      </section>

      <ActionFeedbackPanel feedback={feedback} />
    </div>
  )
}

function AddDeviceView({
  allowDemoControls,
  webSerial,
  feedback,
  onAddDevice,
  selectedAddDeviceKind,
  onLanPaired,
  lanPairing,
  knownDevices,
  onBridgeTargetSelect,
  controlClient,
  devdBaseUrl,
}: {
  allowDemoControls: boolean
  webSerial: Pick<LiveWebSerialControls, 'state' | 'supported'>
  feedback: ActionFeedback
  onAddDevice: (kind: AddDeviceKind) => void
  selectedAddDeviceKind: AddDeviceKind
  onLanPaired: (session: LanDeviceSession, probe: LanProbe) => void | Promise<void>
  lanPairing?: LanPairingOverrides
  knownDevices: DeviceTarget[]
  onBridgeTargetSelect: (device: DeviceTarget) => void
  controlClient: ControlPlaneHttpClient
  devdBaseUrl: string | null
}) {
  return (
    <div className="industrial-view-panel industrial-view-panel--calibration">
      <PanelHeader kicker="Add device" title="Choose connection" />
      <AddDeviceChoices
        allowDemoControls={allowDemoControls}
        webSerial={webSerial}
        onAddDevice={onAddDevice}
        selectedKind={selectedAddDeviceKind}
      />
      {!allowDemoControls && selectedAddDeviceKind === 'wifi' ? (
        <LanPairingPanel {...lanPairing} onPaired={onLanPaired} />
      ) : null}
      {!allowDemoControls && selectedAddDeviceKind === 'bridge' ? (
        <BridgeTargetPanel
          devices={knownDevices}
          onConnect={onBridgeTargetSelect}
          client={controlClient}
          devdBaseUrl={devdBaseUrl}
        />
      ) : null}
      {selectedAddDeviceKind === 'bridge' ? null : <ActionFeedbackPanel feedback={feedback} />}
    </div>
  )
}

type BridgeTransportChoice = 'usb' | 'wifi'
type BridgeConnectionState =
  | { status: 'idle' }
  | { status: 'identifying'; device: DeviceTarget }
  | { status: 'connected'; device: DeviceTarget }
  | { status: 'unknown'; device: DeviceTarget }
  | { status: 'error'; device: DeviceTarget; detail: string }

function BridgeTargetPanel({
  devices,
  onConnect,
  client,
  devdBaseUrl,
}: {
  devices: DeviceTarget[]
  onConnect: (device: DeviceTarget) => void
  client: ControlPlaneHttpClient
  devdBaseUrl: string | null
}) {
  const [transport, setTransport] = useState<BridgeTransportChoice>('usb')
  const [selectedTargetId, setSelectedTargetId] = useState<string | null>(null)
  const [lanDevices, setLanDevices] = useState<DeviceTarget[]>([])
  const [discoveryState, setDiscoveryState] = useState<'idle' | 'loading' | 'error'>('idle')
  const [discoveryError, setDiscoveryError] = useState<string | null>(null)
  const [connectionState, setConnectionState] = useState<BridgeConnectionState>({ status: 'idle' })
  const [cidr, setCidr] = useState(() =>
    typeof window === 'undefined'
      ? ''
      : (window.localStorage.getItem('flux-purr:devd-lan-scan-cidr') ?? '')
  )
  const candidates = bridgeCandidatesForTransport({
    transport,
    devices,
    lanDevices,
  })

  const mergeLanDevices = useCallback((summaries: DevdLanDeviceSummary[]) => {
    setLanDevices((current) => {
      const next = [...current]
      for (const summary of summaries) {
        const target = devdLanSummaryToBridgeTarget(summary)
        const index = next.findIndex((device) => device.id === target.id)
        if (index >= 0) next[index] = target
        else next.push(target)
      }
      return next
    })
  }, [])

  const runDiscovery = async (discover: () => Promise<DevdLanDeviceSummary[]>) => {
    if (discoveryState === 'loading') return
    setDiscoveryState('loading')
    setDiscoveryError(null)
    try {
      mergeLanDevices(await discover())
      setDiscoveryState('idle')
    } catch (error) {
      setDiscoveryState('error')
      setDiscoveryError(error instanceof Error ? error.message : '服务发现失败。')
    }
  }

  useEffect(() => {
    if (transport !== 'wifi' || !devdBaseUrl) return
    let cancelled = false
    void client
      .listDevdLanDevices(devdBaseUrl)
      .then((summaries) => {
        if (!cancelled) mergeLanDevices(summaries)
      })
      .catch(() => undefined)
    return () => {
      cancelled = true
    }
  }, [client, devdBaseUrl, mergeLanDevices, transport])

  const changeTransport = (next: BridgeTransportChoice) => {
    setTransport(next)
    setSelectedTargetId(null)
  }

  const connectUsbCandidate = async (device: DeviceTarget) => {
    if (!devdBaseUrl || connectionState.status === 'identifying') return
    setConnectionState({ status: 'identifying', device })
    let leaseId: string | null = null
    try {
      const lease = await client.createDevdLease(devdBaseUrl, device.id)
      leaseId = lease.leaseId
      const identity = await client.identifyDevdDevice(devdBaseUrl, device.id, lease.leaseId)
      if (!validateBridgeDeviceIdentity(identity).ok) {
        setConnectionState({ status: 'unknown', device })
        return
      }

      const probe = await client.probeDevdDevice(devdBaseUrl, device.id, lease.leaseId)
      const connected = bridgeProbeToDeviceTarget(device, probe)
      setConnectionState({ status: 'connected', device: connected })
    } catch (error) {
      setConnectionState({
        status: 'error',
        device,
        detail: error instanceof Error ? error.message : '无法识别该串口设备。',
      })
    } finally {
      if (leaseId) {
        await client.releaseDevdLease(devdBaseUrl, leaseId).catch(() => undefined)
      }
    }
  }

  const connectLanCandidate = async (device: DeviceTarget) => {
    if (!devdBaseUrl || connectionState.status === 'identifying') return
    setConnectionState({ status: 'identifying', device })
    try {
      const record = await client.connectDevdLanDevice(devdBaseUrl, device.id)
      const registered = devdRecordToDeviceTarget(record)
      if (!validateBridgeDeviceIdentity(record.identity).ok) {
        setConnectionState({ status: 'unknown', device: registered })
        return
      }

      setConnectionState({
        status: 'connected',
        // DEVD's LAN connect endpoint has already read and validated identity,
        // network, and runtime status. Acquiring another DEVD lease here races
        // the live reader in this browser and falsely reports a lease conflict.
        device: bridgeProbeToDeviceTarget(registered, {
          identity: record.identity,
          network: record.network,
          status: record.status,
        }),
      })
    } catch (error) {
      const detail =
        error instanceof ControlPlaneClientError && error.code === 'lan_pairing_required'
          ? '设备拒绝了 DEVD 保存的配对凭据。请在硬件 WiFi Info 页面显示四位码后重新配对。'
          : error instanceof Error
            ? error.message
            : '无法连接该 LAN 设备。'
      setConnectionState({
        status: 'error',
        device,
        detail,
      })
    }
  }

  return (
    <section className="industrial-bridge-target-panel" aria-label="DEVD 桥接目标">
      <div className="industrial-bridge-target-panel__heading">
        <div>
          <strong>本机 DEVD 桥接</strong>
          <small>选择连接路径和具体设备后再建立控制会话</small>
        </div>
        <div className="industrial-bridge-target-panel__transport">
          <Button
            type="button"
            variant="outline"
            aria-pressed={transport === 'usb'}
            onClick={() => changeTransport('usb')}
          >
            <Usb aria-hidden="true" />
            USB
          </Button>
          <Button
            type="button"
            variant="outline"
            aria-pressed={transport === 'wifi'}
            onClick={() => changeTransport('wifi')}
          >
            <Wifi aria-hidden="true" />
            WiFi / LAN
          </Button>
        </div>
      </div>

      {candidates.length > 0 ? (
        <div className="industrial-bridge-target-panel__devices">
          {candidates.map((device) => {
            const selected = selectedTargetId === device.id
            const identifying =
              connectionState.status === 'identifying' && connectionState.device.id === device.id
            return (
              <div key={device.id} className={selected ? 'is-selected' : undefined}>
                <span>
                  <strong>{device.alias}</strong>
                  <small>{device.location}</small>
                </span>
                <Button
                  type="button"
                  variant="outline"
                  disabled={connectionState.status === 'identifying'}
                  onClick={() => {
                    setSelectedTargetId(device.id)
                    if (transport === 'usb') void connectUsbCandidate(device)
                    else void connectLanCandidate(device)
                  }}
                >
                  {identifying ? (
                    <LoaderCircle aria-hidden="true" className="animate-spin" />
                  ) : null}
                  {identifying ? '识别中' : '连接'}
                </Button>
              </div>
            )
          })}
        </div>
      ) : (
        <p className="industrial-bridge-target-panel__empty">
          {transport === 'usb'
            ? 'DEVD 尚未发现可用的 USB 设备。'
            : 'DEVD 尚未发现已登记的 WiFi / LAN 设备。'}
        </p>
      )}

      {transport === 'wifi' ? (
        <div className="industrial-bridge-target-panel__discovery">
          <div className="industrial-bridge-target-panel__discovery-heading">
            <span>
              <strong>服务发现</strong>
              <small>由本机 DEVD 显式执行，不会后台扫描网络</small>
            </span>
            <Button
              type="button"
              variant="outline"
              disabled={!devdBaseUrl || discoveryState === 'loading'}
              onClick={() =>
                devdBaseUrl && runDiscovery(() => client.refreshDevdLanMdns(devdBaseUrl))
              }
            >
              <RefreshCw
                aria-hidden="true"
                className={discoveryState === 'loading' ? 'animate-spin' : undefined}
              />
              刷新服务
            </Button>
          </div>
          <form
            className="industrial-bridge-target-panel__cidr"
            onSubmit={(event) => {
              event.preventDefault()
              const value = cidr.trim()
              if (!devdBaseUrl || !value || discoveryState === 'loading') return
              window.localStorage.setItem('flux-purr:devd-lan-scan-cidr', value)
              void runDiscovery(() => client.scanDevdLanCidr(devdBaseUrl, value))
            }}
          >
            <Label htmlFor="devd-lan-cidr">CIDR 网段</Label>
            <Input
              id="devd-lan-cidr"
              value={cidr}
              placeholder="192.168.31.0/24"
              disabled={!devdBaseUrl || discoveryState === 'loading'}
              onChange={(event) => setCidr(event.target.value)}
            />
            <Button
              type="submit"
              disabled={!devdBaseUrl || !cidr.trim() || discoveryState === 'loading'}
            >
              <ScanSearch aria-hidden="true" />
              扫描网段
            </Button>
          </form>
          {discoveryError ? <p role="alert">{discoveryError}</p> : null}
        </div>
      ) : null}

      {connectionState.status !== 'idle' ? (
        <div className="industrial-bridge-connection-dialog-backdrop">
          <section
            className="industrial-bridge-connection-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="bridge-connection-title"
            aria-describedby="bridge-connection-detail"
          >
            <div className="industrial-bridge-connection-dialog__heading">
              <div>
                <strong id="bridge-connection-title">
                  {connectionState.status === 'identifying'
                    ? '正在识别设备'
                    : connectionState.status === 'connected'
                      ? '设备已连接'
                      : connectionState.status === 'unknown'
                        ? '未知设备'
                        : '无法连接设备'}
                </strong>
                <small>{connectionState.device.location}</small>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                aria-label="关闭连接状态"
                disabled={connectionState.status === 'identifying'}
                onClick={() => setConnectionState({ status: 'idle' })}
              >
                <X aria-hidden="true" />
              </Button>
            </div>
            <div
              className={`industrial-bridge-connection-dialog__status is-${connectionState.status}`}
              aria-live="polite"
            >
              {connectionState.status === 'identifying' ? (
                <LoaderCircle aria-hidden="true" className="animate-spin" />
              ) : connectionState.status === 'connected' ? (
                <CheckCircle2 aria-hidden="true" />
              ) : (
                <AlertTriangle aria-hidden="true" />
              )}
              <p id="bridge-connection-detail">
                {connectionState.status === 'identifying'
                  ? '正在读取固件身份并核对控制协议。'
                  : connectionState.status === 'connected'
                    ? `${connectionState.device.alias} 已通过身份验证。`
                    : connectionState.status === 'unknown'
                      ? '该串口未返回有效的 Flux Purr 固件身份，已禁止连接。'
                      : connectionState.detail}
              </p>
            </div>
            {connectionState.status !== 'identifying' ? (
              <div className="industrial-bridge-connection-dialog__actions">
                <Button
                  type="button"
                  onClick={() => {
                    if (connectionState.status === 'connected') {
                      onConnect(connectionState.device)
                    }
                    setConnectionState({ status: 'idle' })
                  }}
                >
                  {connectionState.status === 'connected' ? '完成' : '关闭'}
                </Button>
              </div>
            ) : null}
          </section>
        </div>
      ) : null}
    </section>
  )
}

export function devdLanSummaryToBridgeTarget(summary: DevdLanDeviceSummary): DeviceTarget {
  const pending = createPendingDevice('bridge')
  return {
    ...pending,
    id: summary.id,
    alias: summary.hostname || summary.id.replace(/^lan-/, ''),
    location: summary.lastIpv4 || summary.baseUrl,
    transport: 'devd',
    bridgeTransport: 'wifi',
    connectionCandidate: true,
    baseUrl: summary.baseUrl,
    severity: summary.paired ? 'nominal' : 'warning',
    leaseState: 'none',
    transportIssue: summary.paired
      ? 'DEVD LAN target is registered and ready to establish a control lease.'
      : '此设备尚未在 DEVD 中完成配对。',
  }
}

function AddDeviceChoices({
  allowDemoControls,
  webSerial,
  onAddDevice,
  selectedKind,
}: {
  allowDemoControls: boolean
  webSerial: Pick<LiveWebSerialControls, 'state' | 'supported'>
  onAddDevice: (kind: AddDeviceKind) => void
  selectedKind?: AddDeviceKind
}) {
  const webSerialDisabled =
    !allowDemoControls && (webSerial.state === 'unsupported' || webSerial.state === 'connecting')

  return (
    <div className="industrial-add-device-grid">
      {addDeviceOptions.map((item) => {
        const disabled = item.kind === 'web-serial' && webSerialDisabled
        const isSelected = item.kind === selectedKind
        const label =
          item.kind === 'web-serial' && webSerial.state === 'connecting'
            ? 'Web Serial (connecting)'
            : item.kind === 'web-serial' && !allowDemoControls && !webSerial.supported
              ? 'Web Serial unavailable'
              : item.label

        return (
          <button
            key={item.kind}
            type="button"
            className={`industrial-add-device-option${isSelected ? ' is-selected' : ''}`}
            aria-pressed={isSelected}
            disabled={disabled}
            onClick={(event) => {
              onAddDevice(item.kind)
              if (event.detail !== 0) {
                event.currentTarget.blur()
              }
            }}
          >
            <span>{label}</span>
            <small>{item.detail}</small>
          </button>
        )
      })}
    </div>
  )
}

function DashboardView({
  device,
  artifact,
  feedback,
  onTargetTempChange,
  onManualPpsApply,
  onManualPpsClear,
  onHeaterHoldToggle,
  onFaultAttentionAcknowledge,
}: {
  device: DeviceTarget
  artifact?: FirmwareArtifact
  feedback: ActionFeedback
  onTargetTempChange: (nextTargetTemp: number) => void
  onManualPpsApply: (millivolts: number) => void | Promise<void>
  onManualPpsClear: () => void | Promise<void>
  onHeaterHoldToggle: () => void
  onFaultAttentionAcknowledge: () => void | Promise<void>
}) {
  const [advancedOpen, setAdvancedOpen] = useState(false)
  const manualPpsDefaultMv = defaultManualPpsMv(device)
  const [manualPpsDraftMv, setManualPpsDraftMv] = useState(() => manualPpsDefaultMv)
  const [manualPpsDraftDirty, setManualPpsDraftDirty] = useState(false)
  const manualPpsDeviceIdRef = useRef(device.id)
  useEffect(() => {
    const deviceChanged = manualPpsDeviceIdRef.current !== device.id
    manualPpsDeviceIdRef.current = device.id
    if (!deviceChanged && advancedOpen && manualPpsDraftDirty) {
      return
    }

    setManualPpsDraftMv(manualPpsDefaultMv)
    setManualPpsDraftDirty(false)
  }, [advancedOpen, device.id, manualPpsDefaultMv, manualPpsDraftDirty])
  const heaterState = runtimeHeaterState(device)
  const powerCapabilityMa = effectivePpsCurrentCapabilityMa(device) ?? 0
  const controlsBlocked = deviceControlBlockReason(device) != null
  return (
    <div className="industrial-view-panel">
      <PanelHeader kicker="Dashboard" title="Thermal runtime" />
      <div className="industrial-runtime-surface">
        <section className={`industrial-temp-dial is-${temperatureBand(device.currentTempC)}`}>
          <p className="industrial-label">Current temp</p>
          <div className="industrial-temp-dial__value">
            <strong>{formatTempNumber(device.currentTempC)}</strong>
            {isRenderableTemperature(device.currentTempC) ? <span>℃</span> : null}
          </div>
          <meter
            className="industrial-heat-output"
            aria-label="Heater output"
            value={device.heaterOutputPercent}
            min={0}
            max={100}
          >
            <span style={{ width: `${device.heaterOutputPercent}%` }} />
          </meter>
          <small>Heater {device.heaterOutputPercent}%</small>
        </section>

        <div className="industrial-signal-stack">
          <StatusCard
            label="PD contract"
            value={formatVolts(device.pdContractMv)}
            detail={`${formatVolts(device.pdRequestMv)} requested / ${formatPdCapability(powerCapabilityMa)} / ${pdStateLabel(device.pdState)}`}
          />
          <StatusCard
            label="Cooling"
            value={device.fanState}
            detail={device.activeCoolingEnabled ? 'Active cooling enabled' : 'Cooling disabled'}
          />
        </div>
      </div>

      <ManualPpsPanel
        device={device}
        open={advancedOpen}
        valueMv={manualPpsDraftMv}
        onOpenChange={setAdvancedOpen}
        onValueChange={(millivolts) => {
          setManualPpsDraftMv(millivolts)
          setManualPpsDraftDirty(true)
        }}
        onApply={() => onManualPpsApply(manualPpsDraftMv)}
        onClear={async () => {
          await onManualPpsClear()
          setManualPpsDraftDirty(false)
        }}
      />

      <div className="industrial-secondary-actions">
        <TargetTempControl
          value={device.targetTempC}
          disabled={controlsBlocked}
          onChange={onTargetTempChange}
        />
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
          disabled={controlsBlocked}
          onClick={onHeaterHoldToggle}
        >
          <Power size={16} aria-hidden="true" />
          {device.heaterEnabled ? 'Hold heater' : 'Resume heater'}
        </button>
        {device.faultAttentionPending ? (
          <button
            type="button"
            className="industrial-button industrial-button--secondary"
            disabled={controlsBlocked}
            onClick={onFaultAttentionAcknowledge}
          >
            消告警
          </button>
        ) : null}
        <RuntimeMiniStatus device={device} artifact={artifact} heaterState={heaterState} />
      </div>
      <CapabilityStrip device={device} />
      <ActionFeedbackPanel feedback={feedback} />
    </div>
  )
}

function ManualPpsPanel({
  device,
  open,
  valueMv,
  onOpenChange,
  onValueChange,
  onApply,
  onClear,
}: {
  device: DeviceTarget
  open: boolean
  valueMv: number
  onOpenChange: (open: boolean) => void
  onValueChange: (millivolts: number) => void
  onApply: () => void | Promise<void>
  onClear: () => void | Promise<void>
}) {
  const range = ppsCapabilityRange(device)
  const maxMa = effectivePpsCurrentCapabilityMa(device)
  const controlsBlocked = deviceControlBlockReason(device) != null
  const disabled = controlsBlocked || !range || maxMa == null
  const clearDisabled = controlsBlocked || !device.manualPpsEnabled
  const capabilityText = range
    ? `${formatVolts(range.minMv)}-${formatVolts(range.maxMv)} / ${maxMa ? formatAmps(maxMa) : 'current unknown'} source range`
    : 'No PPS APDO reported'
  const statusText = device.manualPpsEnabled
    ? `Manual ${formatVolts(device.manualPpsMv ?? valueMv)}`
    : 'Automatic'
  return (
    <section className={open ? 'industrial-advanced is-open' : 'industrial-advanced'}>
      <button
        type="button"
        className="industrial-advanced__toggle"
        aria-expanded={open}
        onClick={() => onOpenChange(!open)}
      >
        <span className="industrial-advanced__icon" aria-hidden="true">
          <SlidersHorizontal size={16} />
        </span>
        <span className="industrial-advanced__summary">
          <span>
            <strong>Advanced PPS</strong>
            <small>{capabilityText}</small>
          </span>
          <span
            className={
              device.manualPpsEnabled
                ? 'industrial-advanced__state is-warning'
                : 'industrial-advanced__state'
            }
          >
            {statusText}
          </span>
        </span>
        <ChevronDown className="industrial-advanced__chevron" size={16} aria-hidden="true" />
      </button>

      {open ? (
        <div className="industrial-advanced__body">
          <div className="industrial-pps-readout">
            <p className="industrial-label">PPS debug</p>
            <strong>{formatVolts(valueMv)}</strong>
            <span>
              {maxMa != null
                ? `PD current capability ${formatAmps(maxMa)}`
                : 'PD current capability unavailable'}
            </span>
          </div>
          <div className="industrial-pps-control">
            <label htmlFor="manual-pps-slider">
              <span>Voltage request</span>
              <strong>{formatVolts(valueMv)}</strong>
            </label>
            <input
              id="manual-pps-slider"
              type="range"
              min={range?.minMv ?? PPS_STEP_MV}
              max={range?.maxMv ?? PPS_HARDWARE_MAX_MV}
              step={PPS_STEP_MV}
              value={valueMv}
              disabled={disabled}
              aria-label="Manual PPS voltage"
              onChange={(event) => onValueChange(Number(event.currentTarget.value))}
            />
            <div className="industrial-pps-control__bounds">
              <span>{range ? formatVolts(range.minMv) : 'No PPS APDO'}</span>
              <span>{range ? formatVolts(range.maxMv) : 'Unavailable'}</span>
            </div>
          </div>
          <div className="industrial-advanced__actions">
            <button
              type="button"
              className="industrial-button industrial-button--secondary"
              disabled={disabled}
              onClick={onApply}
            >
              Apply PPS
            </button>
            <button
              type="button"
              className="industrial-button industrial-button--secondary"
              disabled={clearDisabled}
              onClick={onClear}
            >
              Clear
            </button>
          </div>
          <p className="industrial-advanced__warning">
            <AlertTriangle size={14} aria-hidden="true" />
            <span>
              Manual PPS pauses automatic voltage requests. Current remains read-only and comes from
              device/source telemetry.
            </span>
          </p>
          {device.manualPpsError ? (
            <p className="industrial-advanced__error">Last PPS error: {device.manualPpsError}</p>
          ) : null}
        </div>
      ) : null}
    </section>
  )
}

function TargetTempControl({
  value,
  label = 'Target',
  ariaLabel = 'Dashboard target temperature',
  inputId = 'dashboard-target-temperature',
  inputName = 'dashboardTargetTemperature',
  disabled = false,
  onChange,
}: {
  value: number
  label?: string
  ariaLabel?: string
  inputId?: string
  inputName?: string
  disabled?: boolean
  onChange: (nextTargetTemp: number) => void
}) {
  const applyInputValue = (rawValue: string) => {
    const nextValue = Number(rawValue)

    if (Number.isFinite(nextValue)) {
      onChange(nextValue)
    }
  }

  return (
    <div className="industrial-setpoint-control">
      <div>
        <p className="industrial-label">{label}</p>
        <span>
          {TARGET_TEMP_MIN}-{TARGET_TEMP_MAX}℃
        </span>
      </div>
      <div className="industrial-setpoint-stepper">
        <button
          type="button"
          aria-label="Decrease target temperature"
          disabled={disabled || value <= TARGET_TEMP_MIN}
          onClick={() => onChange(value - TARGET_TEMP_STEP)}
        >
          <Minus size={16} aria-hidden="true" />
        </button>
        <label>
          <span className="sr-only">{ariaLabel}</span>
          <input
            id={inputId}
            name={inputName}
            type="number"
            inputMode="numeric"
            min={TARGET_TEMP_MIN}
            max={TARGET_TEMP_MAX}
            step={TARGET_TEMP_STEP}
            value={Math.round(value)}
            disabled={disabled}
            aria-label={ariaLabel}
            onFocus={(event) => event.currentTarget.select()}
            onChange={(event) => applyInputValue(event.currentTarget.value)}
          />
        </label>
        <button
          type="button"
          aria-label="Increase target temperature"
          disabled={disabled || value >= TARGET_TEMP_MAX}
          onClick={() => onChange(value + TARGET_TEMP_STEP)}
        >
          <Plus size={16} aria-hidden="true" />
        </button>
      </div>
    </div>
  )
}

function CalibrationSliderInputField({
  label,
  valueText,
  unit,
  min,
  max,
  step,
  disabled,
  inputAriaLabel,
  sliderAriaLabel,
  onChange,
  formatBound,
  error,
}: {
  label: string
  valueText: string
  unit: string
  min: number
  max: number
  step: number
  disabled?: boolean
  inputAriaLabel: string
  sliderAriaLabel: string
  onChange: (value: string) => void
  formatBound?: (value: number) => string
  error?: string | null
}) {
  const sliderValue = resolveCalibrationSliderValue(valueText, min, max)
  const displayValueText = valueText.trim() === '' ? String(sliderValue) : valueText
  const errorId = `${inputAriaLabel.replaceAll(/\s+/g, '-')}-error`

  return (
    <div
      className={cn(
        'industrial-calibration-field industrial-calibration-slider-field',
        error && 'industrial-calibration-slider-field--invalid'
      )}
    >
      <div className="industrial-calibration-slider-field__header">
        <span>{label}</span>
        <span className="industrial-calibration-input industrial-calibration-input--compact">
          <input
            type="number"
            inputMode="numeric"
            step={step}
            min={min}
            max={max}
            value={displayValueText}
            disabled={disabled}
            aria-label={inputAriaLabel}
            aria-invalid={error ? true : undefined}
            aria-describedby={error ? errorId : undefined}
            onChange={(event) => onChange(event.currentTarget.value)}
          />
          <small>{unit}</small>
          {error ? (
            <span id={errorId} className="industrial-calibration-input-tooltip" role="alert">
              {error}
            </span>
          ) : null}
        </span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={sliderValue}
        disabled={disabled}
        aria-label={sliderAriaLabel}
        className="industrial-calibration-slider"
        onChange={(event) => onChange(event.currentTarget.value)}
      />
      <div className="industrial-calibration-slider-field__bounds">
        <span>{formatBound ? formatBound(min) : String(min)}</span>
        <span>{formatBound ? formatBound(max) : String(max)}</span>
      </div>
    </div>
  )
}

function CalibrationModeControlPanel({
  title,
  modeToggle,
  leaveGuard,
  capability,
  voltageText,
  onVoltageChange,
  range,
  hasPpsCapability,
  children,
  errors,
  actionSlots,
}: {
  title: string
  modeToggle?: ReactNode
  leaveGuard?: {
    anchorId?: string
    nextLabel: string
    onDismiss: () => void
    onContinue: () => void
  } | null
  capability: ReturnType<typeof calibrationPowerCapability>
  voltageText: string
  onVoltageChange: (value: string) => void
  range: ReturnType<typeof ppsCapabilityRange>
  hasPpsCapability: boolean
  children?: ReactNode
  errors?: ReactNode
  actionSlots: Array<{ id: string; node: ReactNode } | null>
}) {
  const visibleActionSlots = actionSlots.filter(
    (slot): slot is { id: string; node: ReactNode } => slot != null
  )

  return (
    <CalibrationLiveCard
      title={title}
      modeToggle={modeToggle}
      modeToggleAnchorId={leaveGuard ? 'calibration-mode-toggle-anchor' : undefined}
      modeToggleHint={
        leaveGuard ? (
          <CalibrationLeaveGuardBubble
            anchorId="calibration-mode-toggle-anchor"
            nextLabel={leaveGuard.nextLabel}
            onDismiss={leaveGuard.onDismiss}
            onContinue={leaveGuard.onContinue}
          />
        ) : null
      }
      titleMeta={<CalibrationCapabilityHint capability={capability} />}
    >
      <PpsCalibrationFields
        voltageText={voltageText}
        onVoltageChange={onVoltageChange}
        range={range}
        disabled={!hasPpsCapability}
      />
      {children ? <div className="industrial-calibration-live-fields">{children}</div> : null}
      {errors ?? null}
      {visibleActionSlots.length > 0 ? (
        <div className="industrial-calibration-inline-actions industrial-calibration-inline-actions--single-row">
          {visibleActionSlots.map((slot) => (
            <div key={slot.id} className="industrial-calibration-inline-actions__slot">
              {slot.node}
            </div>
          ))}
        </div>
      ) : null}
    </CalibrationLiveCard>
  )
}

function RuntimeMiniStatus({
  device,
  artifact,
  heaterState,
}: {
  device: DeviceTarget
  artifact?: FirmwareArtifact
  heaterState: string
}) {
  return (
    <div className="industrial-runtime-mini">
      <div>
        <p className="industrial-label">Runtime</p>
        <strong>{heaterState}</strong>
      </div>
      <span>
        <Fan size={14} aria-hidden="true" />
        {device.fanState}
      </span>
      <span>{artifact?.version ?? device.firmware}</span>
      {device.faultAttentionPending ? <span>fault attention pending</span> : null}
      {device.heaterLockReason ? (
        <span>{heaterLockReasonText(device.heaterLockReason)}</span>
      ) : null}
    </div>
  )
}

function CapabilityStrip({ device }: { device: DeviceTarget }) {
  const capabilities = [
    ['status', 'Status'],
    ['monitor', 'Monitor'],
    ['flash', 'Flash'],
  ] as const

  return (
    <section className="industrial-capability-strip" aria-label="Transport capabilities">
      {capabilities.map(([capability, label]) => (
        <span
          key={capability}
          className={device.capabilities.includes(capability) ? 'is-enabled' : 'is-disabled'}
        >
          {label}
        </span>
      ))}
      <strong>{device.networkState ?? 'unknown'}</strong>
      {device.transportIssue ? <em>{device.transportIssue}</em> : null}
      {device.heaterLockReason ? <em>{heaterLockReasonText(device.heaterLockReason)}</em> : null}
    </section>
  )
}

function SettingsView({
  device,
  fanPolicyValue,
  selectedPresetIndex,
  presetTemps,
  presetEnabled,
  feedback,
  onPresetSlotChange,
  onPresetTempChange,
  onPresetEnabledChange,
  onFanPolicyChange,
  onWifiSave,
  onWifiClear,
}: {
  device: DeviceTarget
  fanPolicyValue: DeviceTarget['fanState']
  selectedPresetIndex: number
  presetTemps: number[]
  presetEnabled: boolean[]
  feedback: ActionFeedback
  onPresetSlotChange: (presetIndex: number) => void | Promise<void>
  onPresetTempChange: (nextTempC: number) => void | Promise<void>
  onPresetEnabledChange: (nextEnabled: boolean) => void | Promise<void>
  onFanPolicyChange: (fanState: DeviceTarget['fanState']) => void
  onWifiSave: (draft: WifiNetworkSettingsDraft) => Promise<NetworkSummary>
  onWifiClear: () => Promise<NetworkSummary>
}) {
  const hasWifiConfiguration =
    device.transport === 'devd' && device.capabilities.includes('wifi_config')
  const supportsWifiStateV2 = device.capabilities.includes('wifi_state_v2')

  return (
    <div className="industrial-view-panel">
      <PanelHeader kicker="Settings" title="Heat policy" />
      <div className="industrial-settings-stack industrial-settings-stack--distilled">
        <section className="industrial-settings-section industrial-settings-section--summary">
          <div className="industrial-settings-summary">
            <div>
              <span>{formatTemp(device.targetTempC)}</span>
              <small>Live target</small>
            </div>
            <div>
              <span>M{selectedPresetIndex + 1}</span>
              <small>
                {formatPresetTemp(
                  presetTemps[selectedPresetIndex],
                  presetEnabled[selectedPresetIndex] ?? true
                )}{' '}
                {presetEnabled[selectedPresetIndex] ? 'enabled' : 'disabled'}
              </small>
            </div>
          </div>
        </section>

        {hasWifiConfiguration ? (
          <WifiNetworkSettings
            key={device.id}
            deviceId={device.id}
            networkState={device.networkState}
            savedSsid={device.wifiSsid}
            wifiRssi={device.wifiRssi}
            savedPasswordLength={device.wifiPasswordLength ?? 0}
            configurationGeneration={device.configurationGeneration}
            transitionSequence={device.transitionSequence}
            failureCode={device.wifiFailureCode}
            disabled={!supportsWifiStateV2 || !device.leaseId || device.leaseState !== 'active'}
            unavailableReason={resolveWifiSettingsUnavailableReason({
              supportsWifiStateV2,
              transportIssue: device.transportIssue,
            })}
            onSave={onWifiSave}
            onClear={onWifiClear}
          />
        ) : null}

        <section className="industrial-settings-section industrial-settings-section--presets">
          <h3 className="industrial-section-title">Preset temperatures</h3>
          <PresetTemperatureEditor
            selectedPresetIndex={selectedPresetIndex}
            presetTemps={presetTemps}
            presetEnabled={presetEnabled}
            onPresetSlotChange={onPresetSlotChange}
            onPresetTempChange={onPresetTempChange}
            onPresetEnabledChange={onPresetEnabledChange}
          />
        </section>

        <section className="industrial-settings-section industrial-settings-section--controls">
          <h3 className="industrial-section-title">Fan policy</h3>
          <div className="industrial-settings-grid industrial-settings-grid--controls">
            <SegmentedSetting
              label="Fan policy"
              value={fanPolicyValue}
              onChange={onFanPolicyChange}
              hideLabel
            />
          </div>
        </section>
      </div>
      <ActionFeedbackPanel feedback={feedback} compact />
    </div>
  )
}

function CalibrationView({
  navigation,
  device,
  calibration,
  heaterCurve,
  runtimeCalibration,
  refs,
  feedback,
  calibrationWorkspaceTab,
  onTargetTempChange,
  onReferenceChange,
  onCapture,
  onDelete,
  onImport,
  onCalibrationSetActiveSlot,
  onCalibrationSetSlotFit,
  onModeEnter,
  onModeExit,
  onCalibrationRuntimeChange,
  onCalibrationJobChange,
  onHeaterCurvePreview,
  onHeaterCurveClearPreview,
  onHeaterCurveSave,
  onCalibrationWorkspaceTabChange,
  calibrationLeaveGuard,
  onCalibrationLeaveGuardDismiss,
  onCalibrationLeaveGuardClear,
}: {
  navigation?: ConsoleNavigationAdapter
  device: DeviceTarget
  calibration: CalibrationState
  heaterCurve: HeaterCurveState
  runtimeCalibration: CalibrationRuntimeState
  refs: { rtdTempC: number; vinMv: number }
  feedback: ActionFeedback
  calibrationWorkspaceTab: CalibrationWorkspaceTab
  onTargetTempChange: (nextTargetTemp: number) => void
  onReferenceChange: (channel: CalibrationChannel, value: number) => void
  onCapture: (
    channel: CalibrationChannel,
    options?: { referenceValue?: number; targetAdcMv?: number }
  ) => void | Promise<void>
  onDelete: (channel: CalibrationChannel, sampleIndex: number) => void | Promise<void>
  onImport: (calibrationState: CalibrationState) => void | Promise<void>
  onCalibrationSetActiveSlot: (
    channel: CalibrationChannel,
    slot: CalibrationSlotId
  ) => void | Promise<void>
  onCalibrationSetSlotFit: (
    channel: CalibrationChannel,
    slot: CalibrationSlotId,
    fit: CalibrationSlotFit
  ) => void | Promise<void>
  onModeEnter: (
    mode: CalibrationWorkbenchMode,
    request: CalibrationControlRequest
  ) => void | Promise<void>
  onModeExit: () => boolean | Promise<boolean>
  onCalibrationRuntimeChange: (
    request: Partial<CalibrationControlRequest>,
    failureMessage: string
  ) => void | Promise<void>
  onCalibrationJobChange: (
    request: {
      op: 'start' | 'cancel'
      kind?: 'vin_adc_auto' | 'thermal_plant_auto'
    },
    failureMessage: string
  ) => void | Promise<void>
  onHeaterCurvePreview: (heaterCurve: HeaterCurvePackage) => void | Promise<void>
  onHeaterCurveClearPreview: () => void | Promise<void>
  onHeaterCurveSave: () => void | Promise<void>
  onCalibrationWorkspaceTabChange: (nextTab: CalibrationWorkspaceTab) => void
  calibrationLeaveGuard: CalibrationLeaveGuardState | null
  onCalibrationLeaveGuardDismiss: () => void
  onCalibrationLeaveGuardClear: () => void
}) {
  const fileInputRef = useRef<HTMLInputElement | null>(null)
  const [heaterCurveDraftText, setHeaterCurveDraftText] = useState('')
  const [vinPpsMvText, setVinPpsMvText] = useState('')
  const [rtdPpsMvText, setRtdPpsMvText] = useState('')
  const [rtdTargetAdcText, setRtdTargetAdcText] = useState('')
  const [heaterPpsMvText, setHeaterPpsMvText] = useState('')
  const latestRefsRef = useRef(refs)
  const latestRtdTargetAdcTextRef = useRef(rtdTargetAdcText)
  const [pendingCalibrationAction, setPendingCalibrationAction] = useState<string | null>(null)
  const [slotEditor, setSlotEditor] = useState<{
    channel: CalibrationChannel
    slot: CalibrationSlotId
    gainText: string
    offsetText: string
  } | null>(null)
  const lastRtdDraftDeviceIdRef = useRef<string | null>(null)
  const lastLiveRtdTargetAdcMvRef = useRef<number | null>(null)
  const rtdTargetAdcCommitTimerRef = useRef<number | null>(null)
  const rtdTargetAdcCommitVersionRef = useRef(0)
  const ppsCommitTimerRef = useRef<number | null>(null)
  const ppsCommitVersionRef = useRef(0)
  const transportBlockedReason = deviceControlBlockReason(device)
  const controlsBlocked = transportBlockedReason != null
  const requestedWorkbenchMode = calibrationWorkspaceTab
  const activeWorkbenchMode = asWorkbenchMode(runtimeCalibration.mode)
  const modeArmed = activeWorkbenchMode === requestedWorkbenchMode
  const ppsRange = ppsCapabilityRange(device)
  const hasPpsCapability = ppsRange != null
  const powerCapability = calibrationPowerCapability(device)
  const basePpsDraft = calibrationPpsDraft(device, runtimeCalibration)

  const exportCalibration = () => {
    const blob = new Blob([JSON.stringify(calibration, null, 2)], {
      type: 'application/json',
    })
    const url = URL.createObjectURL(blob)
    const link = document.createElement('a')
    link.href = url
    link.download = `${device.id}-adc-calibration.json`
    link.click()
    URL.revokeObjectURL(url)
  }
  const importFile = async (file: File | null) => {
    if (!file) {
      return
    }
    const parsed = JSON.parse(await file.text()) as CalibrationState
    if ('rtdAdc' in parsed && 'vinAdc' in parsed) {
      await onImport(parsed)
    }
  }
  useEffect(() => {
    const source = heaterCurve.preview ?? heaterCurve.active
    setHeaterCurveDraftText(JSON.stringify(source, null, 2))
  }, [heaterCurve.active, heaterCurve.preview])

  useEffect(() => {
    setVinPpsMvText(String(basePpsDraft.millivolts))
  }, [basePpsDraft.millivolts])

  useEffect(() => {
    setRtdPpsMvText(String(basePpsDraft.millivolts))
  }, [basePpsDraft.millivolts])

  useEffect(() => {
    setHeaterPpsMvText(String(basePpsDraft.millivolts))
  }, [basePpsDraft.millivolts])

  useEffect(() => {
    if (lastRtdDraftDeviceIdRef.current === device.id) {
      return
    }

    if (rtdTargetAdcCommitTimerRef.current != null) {
      window.clearTimeout(rtdTargetAdcCommitTimerRef.current)
      rtdTargetAdcCommitTimerRef.current = null
    }
    rtdTargetAdcCommitVersionRef.current = 0
    lastRtdDraftDeviceIdRef.current = device.id
    lastLiveRtdTargetAdcMvRef.current = runtimeCalibration.targetAdcMv ?? null
    const nextTargetAdcMv = runtimeCalibration.targetAdcMv ?? device.rtdRawAdcMv
    setRtdTargetAdcText(nextTargetAdcMv != null ? String(nextTargetAdcMv) : '')
  }, [device.id, device.rtdRawAdcMv, runtimeCalibration.targetAdcMv])

  useEffect(() => {
    setRtdTargetAdcText((current) =>
      syncCalibrationDraftText(
        current,
        runtimeCalibration.targetAdcMv ?? null,
        device.rtdRawAdcMv ?? null,
        lastLiveRtdTargetAdcMvRef
      )
    )
  }, [device.rtdRawAdcMv, runtimeCalibration.targetAdcMv])

  useEffect(() => {
    latestRefsRef.current = refs
  }, [refs])

  useEffect(() => {
    latestRtdTargetAdcTextRef.current = rtdTargetAdcText
  }, [rtdTargetAdcText])

  const currentRtdTargetAdcMv = useCallback(() => {
    const parsed = parseCalibrationIntegerInput(latestRtdTargetAdcTextRef.current)
    return parsed ?? runtimeCalibration.targetAdcMv ?? device.rtdRawAdcMv ?? RTD_TARGET_MIN_MV
  }, [device.rtdRawAdcMv, runtimeCalibration.targetAdcMv])

  const currentModeError = runtimeCalibration.error ?? null
  const currentJob = runtimeCalibration.job
  const jobRunning = currentJob.status === 'running'
  const actionLockTimerRef = useRef<number | null>(null)

  const vinPpsMv = parseCalibrationIntegerInput(vinPpsMvText)
  const vinPpsError =
    vinPpsMv == null ? '请输入整数 PPS 电压。' : validateCalibrationPpsInput(device, vinPpsMv)
  const vinCanSubmitPps = hasPpsCapability && vinPpsError == null

  const rtdPpsMv = parseCalibrationIntegerInput(rtdPpsMvText)
  const effectiveRtdTargetAdcMv = resolveCalibrationSliderValue(
    rtdTargetAdcText,
    RTD_TARGET_MIN_MV,
    RTD_TARGET_MAX_MV
  )
  const rtdPpsError =
    rtdPpsMv == null ? '请输入整数 PPS 电压。' : validateCalibrationPpsInput(device, rtdPpsMv)
  const rtdTargetError = validateCalibrationSliderText(
    rtdTargetAdcText,
    RTD_TARGET_MIN_MV,
    RTD_TARGET_MAX_MV
  )
  const rtdHeaterToggleDisabled = controlsBlocked || !modeArmed || pendingCalibrationAction != null

  const heaterPpsMv = parseCalibrationIntegerInput(heaterPpsMvText)
  const heaterPpsError =
    heaterPpsMv == null ? '请输入整数 PPS 电压。' : validateCalibrationPpsInput(device, heaterPpsMv)
  const activePpsMvText =
    calibrationWorkspaceTab === 'heater_curve'
      ? heaterPpsMvText
      : calibrationWorkspaceTab === 'rtd_adc'
        ? rtdPpsMvText
        : vinPpsMvText
  const activePpsMv = parseCalibrationIntegerInput(activePpsMvText)
  const activePpsError =
    calibrationWorkspaceTab === 'heater_curve'
      ? heaterPpsError
      : calibrationWorkspaceTab === 'rtd_adc'
        ? rtdPpsError
        : vinPpsError
  useEffect(
    () => () => {
      if (rtdTargetAdcCommitTimerRef.current != null) {
        window.clearTimeout(rtdTargetAdcCommitTimerRef.current)
      }
      if (ppsCommitTimerRef.current != null) {
        window.clearTimeout(ppsCommitTimerRef.current)
      }
      if (actionLockTimerRef.current != null) {
        window.clearTimeout(actionLockTimerRef.current)
      }
    },
    []
  )

  useEffect(() => {
    if (rtdTargetAdcCommitTimerRef.current != null) {
      window.clearTimeout(rtdTargetAdcCommitTimerRef.current)
      rtdTargetAdcCommitTimerRef.current = null
    }

    if (
      controlsBlocked ||
      pendingCalibrationAction != null ||
      !modeArmed ||
      runtimeCalibration.mode !== 'rtd_adc' ||
      !runtimeCalibration.ppsEnabled ||
      rtdTargetError != null ||
      runtimeCalibration.targetAdcMv === effectiveRtdTargetAdcMv
    ) {
      return
    }

    const nextVersion = rtdTargetAdcCommitVersionRef.current + 1
    rtdTargetAdcCommitVersionRef.current = nextVersion
    rtdTargetAdcCommitTimerRef.current = window.setTimeout(() => {
      rtdTargetAdcCommitTimerRef.current = null
      if (rtdTargetAdcCommitVersionRef.current !== nextVersion) {
        return
      }
      void onCalibrationRuntimeChange(
        {
          targetAdcMv: effectiveRtdTargetAdcMv,
        },
        '目标 ADC 更新失败。'
      )
    }, 180)

    return () => {
      if (rtdTargetAdcCommitTimerRef.current != null) {
        window.clearTimeout(rtdTargetAdcCommitTimerRef.current)
        rtdTargetAdcCommitTimerRef.current = null
      }
    }
  }, [
    controlsBlocked,
    effectiveRtdTargetAdcMv,
    modeArmed,
    onCalibrationRuntimeChange,
    pendingCalibrationAction,
    rtdTargetError,
    runtimeCalibration.mode,
    runtimeCalibration.ppsEnabled,
    runtimeCalibration.targetAdcMv,
  ])

  useEffect(() => {
    if (
      controlsBlocked ||
      pendingCalibrationAction != null ||
      !modeArmed ||
      runtimeCalibration.mode === 'thermal_plant' ||
      runtimeCalibration.mode === 'off' ||
      runtimeCalibration.ppsEnabled ||
      activePpsMv == null ||
      activePpsError != null
    ) {
      return
    }

    void onCalibrationRuntimeChange(
      {
        mode: runtimeCalibration.mode,
        ppsEnabled: true,
        ppsMv: activePpsMv,
        ...(runtimeCalibration.mode === 'rtd_adc' ? { targetAdcMv: currentRtdTargetAdcMv() } : {}),
      },
      'PPS 接管失败。'
    )
  }, [
    activePpsError,
    activePpsMv,
    controlsBlocked,
    currentRtdTargetAdcMv,
    modeArmed,
    onCalibrationRuntimeChange,
    pendingCalibrationAction,
    runtimeCalibration.mode,
    runtimeCalibration.ppsEnabled,
  ])

  useEffect(() => {
    if (ppsCommitTimerRef.current != null) {
      window.clearTimeout(ppsCommitTimerRef.current)
      ppsCommitTimerRef.current = null
    }

    if (
      controlsBlocked ||
      pendingCalibrationAction != null ||
      !modeArmed ||
      runtimeCalibration.mode === 'thermal_plant' ||
      runtimeCalibration.mode === 'off' ||
      !runtimeCalibration.ppsEnabled ||
      activePpsMv == null ||
      activePpsError != null ||
      runtimeCalibration.ppsMv === activePpsMv
    ) {
      return
    }

    const nextVersion = ppsCommitVersionRef.current + 1
    ppsCommitVersionRef.current = nextVersion
    ppsCommitTimerRef.current = window.setTimeout(() => {
      ppsCommitTimerRef.current = null
      if (ppsCommitVersionRef.current !== nextVersion) {
        return
      }
      void onCalibrationRuntimeChange(
        {
          mode: runtimeCalibration.mode,
          ppsEnabled: true,
          ppsMv: activePpsMv,
          ...(runtimeCalibration.mode === 'rtd_adc'
            ? { targetAdcMv: currentRtdTargetAdcMv() }
            : {}),
        },
        'PPS 电压更新失败。'
      )
    }, 180)

    return () => {
      if (ppsCommitTimerRef.current != null) {
        window.clearTimeout(ppsCommitTimerRef.current)
        ppsCommitTimerRef.current = null
      }
    }
  }, [
    activePpsError,
    activePpsMv,
    controlsBlocked,
    currentRtdTargetAdcMv,
    modeArmed,
    onCalibrationRuntimeChange,
    pendingCalibrationAction,
    runtimeCalibration.mode,
    runtimeCalibration.ppsEnabled,
    runtimeCalibration.ppsMv,
  ])

  const runCalibrationAction = useCallback(
    async (actionKey: string, action: () => void | Promise<void>) => {
      if (pendingCalibrationAction != null) {
        return
      }
      setPendingCalibrationAction(actionKey)
      try {
        await action()
      } finally {
        if (actionLockTimerRef.current != null) {
          window.clearTimeout(actionLockTimerRef.current)
        }
        actionLockTimerRef.current = window.setTimeout(() => {
          setPendingCalibrationAction((current) => (current === actionKey ? null : current))
          actionLockTimerRef.current = null
        }, CALIBRATION_ACTION_LOCK_MS)
      }
    },
    [pendingCalibrationAction]
  )

  const calibrationActionPending = (actionKey: string) => pendingCalibrationAction === actionKey
  const openSlotEditor = useCallback(
    (channel: CalibrationChannel, slot: CalibrationSlotId, fit: CalibrationSlotFit) => {
      setSlotEditor({
        channel,
        slot,
        gainText: String(fit.gain),
        offsetText: String(fit.offsetMv),
      })
    },
    []
  )
  const closeSlotEditor = useCallback(() => {
    setSlotEditor(null)
  }, [])
  const applyFittedFitToEditor = useCallback((fit: CalibrationFit) => {
    setSlotEditor((current) =>
      current
        ? {
            ...current,
            gainText: String(fit.gain),
            offsetText: String(fit.offsetMv),
          }
        : current
    )
  }, [])
  const submitSlotEditor = useCallback(async () => {
    if (!slotEditor) {
      return
    }
    const gain = Number(slotEditor.gainText)
    const offsetMv = Number(slotEditor.offsetText)
    if (!Number.isFinite(gain) || !Number.isFinite(offsetMv)) {
      return
    }
    await onCalibrationSetSlotFit(slotEditor.channel, slotEditor.slot, {
      gain,
      offsetMv,
    })
    closeSlotEditor()
  }, [closeSlotEditor, onCalibrationSetSlotFit, slotEditor])
  const leaveGuardViewModel = calibrationLeaveGuard
    ? {
        anchorId: calibrationLeaveGuard.anchorId,
        nextLabel: calibrationLeaveGuard.nextLabel,
        onDismiss: onCalibrationLeaveGuardDismiss,
        onContinue: async () => {
          const continueAction = calibrationLeaveGuard.continueAction
          const exited = await onModeExit()
          if (!exited) {
            return
          }
          onCalibrationLeaveGuardClear()
          await continueAction()
        },
      }
    : null
  const leaveGuardForTab = (tab: CalibrationWorkspaceTab) =>
    calibrationWorkspaceTab === tab ? leaveGuardViewModel : null

  const adcToolbar = (
    <AdcCalibrationToolbar
      disabled={controlsBlocked}
      feedback={feedback}
      onExport={exportCalibration}
      onImport={() => fileInputRef.current?.click()}
    />
  )
  const slotEditorFittedFit =
    slotEditor?.channel === 'rtd_adc'
      ? calibration.rtdAdc.fittedFit
      : slotEditor?.channel === 'vin_adc'
        ? calibration.vinAdc.fittedFit
        : null

  return (
    <div className="industrial-view-panel industrial-view-panel--calibration-workbench">
      <div className="industrial-calibration-workbench">
        <CalibrationSlotEditOverlay
          slotEditor={slotEditor}
          fittedFit={slotEditorFittedFit}
          onChange={(next) =>
            setSlotEditor((current) => (current ? { ...current, ...next } : current))
          }
          onAdoptFit={() =>
            slotEditorFittedFit ? applyFittedFitToEditor(slotEditorFittedFit) : undefined
          }
          onClose={closeSlotEditor}
          onSubmit={() => void submitSlotEditor()}
        />
        <input
          ref={fileInputRef}
          type="file"
          accept="application/json,.json"
          hidden
          onChange={(event) => void importFile(event.currentTarget.files?.[0] ?? null)}
        />
        <Tabs
          value={calibrationWorkspaceTab}
          onValueChange={(value) =>
            onCalibrationWorkspaceTabChange(value as CalibrationWorkspaceTab)
          }
          className="industrial-calibration-tabs"
        >
          <TabsList
            variant="line"
            className="industrial-calibration-tabs__list"
            aria-label="Calibration tools"
          >
            <CalibrationRouteTab navigation={navigation} tab="heater_curve">
              加热曲线标定
            </CalibrationRouteTab>
            <CalibrationRouteTab navigation={navigation} tab="rtd_adc">
              温度标定
            </CalibrationRouteTab>
            <CalibrationRouteTab navigation={navigation} tab="vin_adc">
              电压读数标定
            </CalibrationRouteTab>
          </TabsList>

          <TabsContent value="heater_curve" className="industrial-calibration-tabs__content">
            <section className="industrial-calibration-mode-panel" aria-label="加热曲线标定">
              <div className="industrial-calibration-live-grid industrial-calibration-live-grid--staggered">
                <div className="industrial-calibration-live-stack">
                  <CalibrationModeControlPanel
                    title="校准控制"
                    modeToggle={
                      <CalibrationModeToggle
                        active={modeArmed}
                        disabled={controlsBlocked || jobRunning}
                        onEnable={() =>
                          void onModeEnter('heater_curve', {
                            mode: 'heater_curve',
                            ppsEnabled: true,
                            ppsMv: heaterPpsMv ?? basePpsDraft.millivolts,
                            heaterEnabled: false,
                          })
                        }
                        onDisable={() => onModeExit()}
                      />
                    }
                    leaveGuard={leaveGuardForTab('heater_curve')}
                    capability={powerCapability}
                    voltageText={heaterPpsMvText}
                    onVoltageChange={setHeaterPpsMvText}
                    range={ppsRange}
                    hasPpsCapability={hasPpsCapability}
                    errors={
                      heaterPpsError ? (
                        <p className="industrial-calibration-inline-error">{heaterPpsError}</p>
                      ) : null
                    }
                    actionSlots={[
                      {
                        id: 'thermal-plant-job-toggle',
                        node: (
                          <button
                            type="button"
                            className="industrial-button industrial-button--secondary"
                            disabled={
                              controlsBlocked ||
                              pendingCalibrationAction != null ||
                              (!jobRunning && !hasPpsCapability) ||
                              (jobRunning && currentJob.kind !== 'thermal_plant_auto')
                            }
                            onClick={() =>
                              void runCalibrationAction('thermal-plant-job-toggle', async () => {
                                if (jobRunning) {
                                  await onCalibrationJobChange(
                                    { op: 'cancel' },
                                    '自动热模型标定停止失败。'
                                  )
                                  return
                                }
                                await onCalibrationJobChange(
                                  { op: 'start', kind: 'thermal_plant_auto' },
                                  '自动热模型标定启动失败。'
                                )
                              })
                            }
                          >
                            {calibrationActionPending('thermal-plant-job-toggle')
                              ? '处理中...'
                              : jobRunning
                                ? '停止自动热模型'
                                : '自动热模型'}
                          </button>
                        ),
                      },
                      {
                        id: 'heater-heater-toggle',
                        node: (
                          <CalibrationActionToggle
                            label="加热"
                            active={runtimeCalibration.heaterEnabled}
                            disabled={
                              controlsBlocked ||
                              jobRunning ||
                              !modeArmed ||
                              pendingCalibrationAction != null
                            }
                            onCheckedChange={() =>
                              void runCalibrationAction('heater-heater-toggle', () =>
                                onCalibrationRuntimeChange(
                                  {
                                    mode: 'heater_curve',
                                    heaterEnabled: !runtimeCalibration.heaterEnabled,
                                    ppsEnabled: true,
                                    ppsMv: heaterPpsMv ?? basePpsDraft.millivolts,
                                  },
                                  '加热切换失败。'
                                )
                              )
                            }
                          />
                        ),
                      },
                    ]}
                  >
                    <CalibrationSliderInputField
                      label="目标温度"
                      valueText={String(Math.round(device.targetTempC))}
                      unit="℃"
                      min={TARGET_TEMP_MIN}
                      max={TARGET_TEMP_MAX}
                      step={TARGET_TEMP_STEP}
                      disabled={
                        controlsBlocked ||
                        jobRunning ||
                        !modeArmed ||
                        pendingCalibrationAction != null
                      }
                      inputAriaLabel="加热曲线标定目标温度输入"
                      sliderAriaLabel="加热曲线标定目标温度滑块"
                      onChange={(value) => {
                        const nextValue = Number(value)
                        if (Number.isFinite(nextValue)) {
                          onTargetTempChange(nextValue)
                        }
                      }}
                      formatBound={(value) => `${value}℃`}
                    />
                  </CalibrationModeControlPanel>
                </div>
                <div className="industrial-calibration-side-stack">
                  <HeaterCurveWorkbenchCard
                    device={device}
                    heaterCurve={heaterCurve}
                    draftText={heaterCurveDraftText}
                    disabled={controlsBlocked}
                    currentModeError={currentModeError}
                    currentJobMessage={currentJob.message}
                    runtimeCalibration={runtimeCalibration}
                    onDraftTextChange={setHeaterCurveDraftText}
                    onPreview={onHeaterCurvePreview}
                    onClearPreview={onHeaterCurveClearPreview}
                    onSave={onHeaterCurveSave}
                  />
                  {adcToolbar}
                </div>
              </div>
              <HeaterCurvePanel
                device={device}
                heaterCurve={heaterCurve}
                draftText={heaterCurveDraftText}
                disabled={controlsBlocked}
                onDraftTextChange={setHeaterCurveDraftText}
              />
            </section>
          </TabsContent>
          <TabsContent value="rtd_adc" className="industrial-calibration-tabs__content">
            <section className="industrial-calibration-mode-panel" aria-label="温度标定">
              <div className="industrial-calibration-live-grid industrial-calibration-live-grid--staggered">
                <div className="industrial-calibration-live-stack">
                  <CalibrationModeControlPanel
                    title="校准控制"
                    modeToggle={
                      <CalibrationModeToggle
                        active={modeArmed}
                        disabled={controlsBlocked}
                        onEnable={() =>
                          void onModeEnter('rtd_adc', {
                            mode: 'rtd_adc',
                            ppsEnabled: true,
                            ppsMv: rtdPpsMv ?? basePpsDraft.millivolts,
                            heaterEnabled: false,
                            targetAdcMv: currentRtdTargetAdcMv(),
                          })
                        }
                        onDisable={() => onModeExit()}
                      />
                    }
                    leaveGuard={leaveGuardForTab('rtd_adc')}
                    capability={powerCapability}
                    voltageText={rtdPpsMvText}
                    onVoltageChange={setRtdPpsMvText}
                    range={ppsRange}
                    hasPpsCapability={hasPpsCapability}
                    errors={
                      rtdPpsError ? (
                        <p className="industrial-calibration-inline-error">{rtdPpsError}</p>
                      ) : null
                    }
                    actionSlots={[
                      {
                        id: 'rtd-heater-toggle',
                        node: (
                          <CalibrationActionToggle
                            label="加热"
                            active={runtimeCalibration.heaterEnabled}
                            disabled={rtdHeaterToggleDisabled}
                            onCheckedChange={() =>
                              void runCalibrationAction('rtd-heater-toggle', () =>
                                onCalibrationRuntimeChange(
                                  {
                                    mode: 'rtd_adc',
                                    heaterEnabled: !runtimeCalibration.heaterEnabled,
                                    ppsEnabled: true,
                                    ppsMv: rtdPpsMv ?? basePpsDraft.millivolts,
                                    targetAdcMv: currentRtdTargetAdcMv(),
                                  },
                                  '加热切换失败。'
                                )
                              )
                            }
                          />
                        ),
                      },
                    ]}
                  >
                    <CalibrationSliderInputField
                      label="目标 ADC"
                      valueText={rtdTargetAdcText}
                      unit="mV"
                      min={RTD_TARGET_MIN_MV}
                      max={RTD_TARGET_MAX_MV}
                      step={RTD_TARGET_STEP_MV}
                      disabled={controlsBlocked || !hasPpsCapability}
                      inputAriaLabel="目标 ADC 输入"
                      sliderAriaLabel="目标 ADC 滑块"
                      onChange={setRtdTargetAdcText}
                      error={rtdTargetError}
                    />
                  </CalibrationModeControlPanel>
                </div>
                <div className="industrial-calibration-side-stack">
                  <CalibrationWorkbenchCard
                    title="状态"
                    summary={
                      <>
                        <CalibrationFitStatusSummary
                          liveLabel="当前 ADC"
                          liveValue={
                            device.rtdRawAdcMv != null ? `${device.rtdRawAdcMv}mV` : '未采样'
                          }
                          channelState={calibration.rtdAdc}
                          channel="rtd_adc"
                          disabled={controlsBlocked}
                          onSetActiveSlot={onCalibrationSetActiveSlot}
                          onEditSlot={openSlotEditor}
                        />
                        <CalibrationHeaterFeedback
                          heaterEnabled={runtimeCalibration.heaterEnabled}
                          heaterOutputPercent={device.heaterOutputPercent}
                        />
                      </>
                    }
                  />
                  {adcToolbar}
                </div>
              </div>
              <section className="industrial-calibration-channel industrial-calibration-channel--samples">
                <CalibrationChannelSamples
                  channel="rtd_adc"
                  title="温度 ADC"
                  samples={calibration.rtdAdc.samples}
                  disabled={controlsBlocked}
                  messages={
                    currentModeError ? (
                      <p className="industrial-calibration-inline-error">{currentModeError}</p>
                    ) : null
                  }
                  controls={
                    <div className="industrial-calibration-sample-control-row">
                      <CalibrationFittedSuggestionCard
                        title="温度 ADC 拟合建议"
                        fit={calibration.rtdAdc.fittedFit}
                      />
                      <CalibrationChannelControls
                        referenceLabel="标定温度"
                        referenceValue={refs.rtdTempC}
                        referenceUnit="℃"
                        referenceAsPlaceholder
                        disabled={controlsBlocked}
                        onReferenceChange={(value) => onReferenceChange('rtd_adc', value)}
                        onCapture={(referenceValue) =>
                          onCapture('rtd_adc', {
                            referenceValue,
                            targetAdcMv: currentRtdTargetAdcMv(),
                          })
                        }
                      />
                    </div>
                  }
                  onDelete={(sampleIndex) => onDelete('rtd_adc', sampleIndex)}
                />
              </section>
            </section>
          </TabsContent>
          <TabsContent value="vin_adc" className="industrial-calibration-tabs__content">
            <section className="industrial-calibration-mode-panel" aria-label="电压读数标定">
              <div className="industrial-calibration-live-grid industrial-calibration-live-grid--staggered">
                <div className="industrial-calibration-live-stack">
                  <CalibrationModeControlPanel
                    title="校准控制"
                    modeToggle={
                      <CalibrationModeToggle
                        active={modeArmed}
                        disabled={controlsBlocked}
                        onEnable={() =>
                          void onModeEnter('vin_adc', {
                            mode: 'vin_adc',
                            ppsEnabled: true,
                            ppsMv: vinPpsMv ?? basePpsDraft.millivolts,
                            heaterEnabled: false,
                          })
                        }
                        onDisable={() => onModeExit()}
                      />
                    }
                    leaveGuard={leaveGuardForTab('vin_adc')}
                    capability={powerCapability}
                    voltageText={vinPpsMvText}
                    onVoltageChange={setVinPpsMvText}
                    range={ppsRange}
                    hasPpsCapability={hasPpsCapability}
                    errors={
                      vinPpsError ? (
                        <p className="industrial-calibration-inline-error">{vinPpsError}</p>
                      ) : null
                    }
                    actionSlots={[
                      {
                        id: 'vin-job-toggle',
                        node: (
                          <button
                            type="button"
                            className="industrial-button industrial-button--secondary"
                            disabled={
                              controlsBlocked ||
                              pendingCalibrationAction != null ||
                              (!jobRunning && (!modeArmed || !vinCanSubmitPps))
                            }
                            onClick={() =>
                              void runCalibrationAction('vin-job-toggle', () =>
                                onCalibrationJobChange(
                                  jobRunning
                                    ? { op: 'cancel' }
                                    : { op: 'start', kind: 'vin_adc_auto' },
                                  jobRunning ? '电压自动扫点取消失败。' : '电压自动扫点启动失败。'
                                )
                              )
                            }
                          >
                            {calibrationActionPending('vin-job-toggle')
                              ? '处理中...'
                              : jobRunning
                                ? '停止扫点'
                                : '自动扫点'}
                          </button>
                        ),
                      },
                    ]}
                  >
                    {null}
                  </CalibrationModeControlPanel>
                </div>
                <div className="industrial-calibration-side-stack">
                  <CalibrationWorkbenchCard
                    title="状态"
                    summary={
                      <CalibrationFitStatusSummary
                        liveLabel="当前 ADC"
                        liveValue={
                          device.vinRawAdcMv != null ? `${device.vinRawAdcMv}mV` : '未采样'
                        }
                        channelState={calibration.vinAdc}
                        channel="vin_adc"
                        disabled={controlsBlocked}
                        onSetActiveSlot={onCalibrationSetActiveSlot}
                        onEditSlot={openSlotEditor}
                      />
                    }
                  />
                  {adcToolbar}
                </div>
              </div>
              <section className="industrial-calibration-channel industrial-calibration-channel--samples">
                <CalibrationChannelSamples
                  channel="vin_adc"
                  title="电压 ADC"
                  samples={calibration.vinAdc.samples}
                  disabled={controlsBlocked}
                  messages={
                    <>
                      {currentModeError ? (
                        <p className="industrial-calibration-inline-error">{currentModeError}</p>
                      ) : null}
                      {currentJob.message ? (
                        <p className="industrial-calibration-inline-error">{currentJob.message}</p>
                      ) : null}
                    </>
                  }
                  controls={
                    <div className="industrial-calibration-sample-control-row">
                      <CalibrationFittedSuggestionCard
                        title="电压 ADC 拟合建议"
                        fit={calibration.vinAdc.fittedFit}
                      />
                      <CalibrationChannelControls
                        referenceLabel="参考电压"
                        referenceValue={refs.vinMv}
                        referenceUnit="mV"
                        disabled={controlsBlocked}
                        onReferenceChange={(value) => onReferenceChange('vin_adc', value)}
                        onCapture={(referenceValue) =>
                          onCapture('vin_adc', {
                            referenceValue,
                          })
                        }
                      />
                    </div>
                  }
                  onDelete={(sampleIndex) => onDelete('vin_adc', sampleIndex)}
                />
              </section>
            </section>
          </TabsContent>
        </Tabs>
      </div>
    </div>
  )
}

function CalibrationModeToggle({
  active,
  disabled = false,
  onEnable,
  onDisable,
}: {
  active: boolean
  disabled?: boolean
  onEnable: () => void | Promise<void>
  onDisable: () => boolean | Promise<boolean>
}) {
  return (
    <Switch
      aria-label="标定模式"
      size="industrial"
      className="industrial-calibration-mode-switch"
      checked={active}
      disabled={disabled}
      onCheckedChange={(checked) => void (checked ? onEnable() : onDisable())}
    />
  )
}

function CalibrationActionToggle({
  label,
  active,
  disabled = false,
  onCheckedChange,
}: {
  label: string
  active: boolean
  disabled?: boolean
  onCheckedChange: (checked: boolean) => void | Promise<void>
}) {
  return (
    <div className="industrial-calibration-action-toggle">
      <span>{label}</span>
      <Switch
        aria-label={`${label}开关`}
        size="industrial"
        className="industrial-calibration-action-toggle__switch"
        checked={active}
        disabled={disabled}
        onCheckedChange={(checked) => void onCheckedChange(checked)}
      />
    </div>
  )
}

function CalibrationLiveCard({
  title,
  detail,
  modeToggle,
  modeToggleHint,
  modeToggleAnchorId,
  titleMeta,
  compact = false,
  children,
}: {
  title: string
  detail?: string
  modeToggle?: ReactNode
  modeToggleHint?: ReactNode
  modeToggleAnchorId?: string
  titleMeta?: ReactNode
  compact?: boolean
  children: ReactNode
}) {
  return (
    <section
      className={cn(
        'industrial-calibration-live-card',
        compact && 'industrial-calibration-live-card--compact'
      )}
    >
      <div className="industrial-calibration-live-card__header">
        <div className="industrial-calibration-live-card__title-row">
          <div
            className={
              detail
                ? 'industrial-calibration-live-card__title-block'
                : 'industrial-calibration-live-card__title-block industrial-calibration-live-card__title-block--compact'
            }
          >
            <div className="industrial-calibration-live-card__title-main">
              <h3>{title}</h3>
              {titleMeta ?? null}
            </div>
            {detail ? <p>{detail}</p> : null}
          </div>
          <div className="industrial-calibration-live-card__mode-control">
            {modeToggle ? (
              <div
                id={modeToggleAnchorId}
                className="industrial-calibration-live-card__mode-toggle-anchor"
              >
                {modeToggle}
              </div>
            ) : null}
            {modeToggleHint ?? null}
          </div>
        </div>
      </div>
      {children}
    </section>
  )
}

function CalibrationLeaveGuardBubble({
  anchorId,
  nextLabel,
  onDismiss,
  onContinue,
}: {
  anchorId?: string
  nextLabel: string
  onDismiss: () => void
  onContinue: () => void
}) {
  const anchorRef = useRef<HTMLElement | null>(null)
  const bubbleRef = useRef<HTMLDivElement | null>(null)
  const continueButtonRef = useRef<HTMLButtonElement | null>(null)
  const titleId = useId()
  const descriptionId = useId()
  const [bubbleStyle, setBubbleStyle] = useState<CSSProperties>({
    visibility: 'hidden',
  })
  const [bubbleSide, setBubbleSide] = useState<'bottom' | 'top'>('bottom')

  useEffect(() => {
    const previouslyFocused =
      document.activeElement instanceof HTMLElement ? document.activeElement : null
    continueButtonRef.current?.focus()
    return () => previouslyFocused?.focus()
  }, [])

  useLayoutEffect(() => {
    const anchor = (anchorId ? document.getElementById(anchorId) : null) ?? anchorRef.current
    anchorRef.current = anchor
    if (!anchor) {
      return
    }

    let frameId = 0
    const gap = 10
    const viewportMargin = 16

    const updatePosition = () => {
      const bubble = bubbleRef.current
      if (!bubble) {
        return
      }

      const anchorRect = anchor.getBoundingClientRect()
      const bubbleRect = bubble.getBoundingClientRect()
      let nextSide: 'bottom' | 'top' = 'bottom'
      let left = anchorRect.left + anchorRect.width / 2 - bubbleRect.width / 2
      let top = anchorRect.bottom + gap

      if (top + bubbleRect.height > window.innerHeight - viewportMargin) {
        nextSide = 'top'
        top = anchorRect.top - bubbleRect.height - gap
      }

      left = Math.min(
        Math.max(viewportMargin, left),
        Math.max(viewportMargin, window.innerWidth - bubbleRect.width - viewportMargin)
      )
      top = Math.min(
        Math.max(viewportMargin, top),
        Math.max(viewportMargin, window.innerHeight - bubbleRect.height - viewportMargin)
      )

      setBubbleSide(nextSide)
      setBubbleStyle({
        left,
        top,
        visibility: 'visible',
      })
    }

    const scheduleUpdate = () => {
      if (frameId !== 0) {
        window.cancelAnimationFrame(frameId)
      }
      frameId = window.requestAnimationFrame(() => {
        frameId = 0
        updatePosition()
      })
    }

    scheduleUpdate()
    window.addEventListener('resize', scheduleUpdate)
    window.addEventListener('scroll', scheduleUpdate, true)

    const observer =
      typeof ResizeObserver === 'function' ? new ResizeObserver(() => scheduleUpdate()) : null
    observer?.observe(anchor)
    if (bubbleRef.current) {
      observer?.observe(bubbleRef.current)
    }

    return () => {
      if (frameId !== 0) {
        window.cancelAnimationFrame(frameId)
      }
      observer?.disconnect()
      window.removeEventListener('resize', scheduleUpdate)
      window.removeEventListener('scroll', scheduleUpdate, true)
    }
  }, [anchorId])

  const bubble =
    typeof document === 'undefined'
      ? null
      : createPortal(
          <div
            ref={bubbleRef}
            className="industrial-calibration-leave-guard"
            data-side={bubbleSide}
            role="dialog"
            aria-modal="false"
            aria-labelledby={titleId}
            aria-describedby={descriptionId}
            style={bubbleStyle}
          >
            <div className="industrial-calibration-leave-guard__header">
              <div className="industrial-calibration-leave-guard__badge">
                <AlertTriangle size={12} strokeWidth={2.3} aria-hidden="true" />
                <span id={titleId}>校准未关闭</span>
              </div>
            </div>
            <p id={descriptionId}>校准控制仍开着，先关闭后再切到“{nextLabel}”。</p>
            <div className="industrial-calibration-leave-guard__actions">
              <button
                ref={continueButtonRef}
                type="button"
                className="industrial-button industrial-button--secondary"
                onClick={onContinue}
              >
                关闭并继续
              </button>
              <button
                type="button"
                className="industrial-button industrial-button--ghost"
                onClick={onDismiss}
              >
                留在当前页
              </button>
            </div>
          </div>,
          document.body
        )

  return (
    <>
      <span
        ref={anchorRef}
        className="industrial-calibration-leave-guard-anchor"
        aria-hidden="true"
      />
      {bubble}
    </>
  )
}

function CalibrationCapabilityHint({
  capability,
}: {
  capability: ReturnType<typeof calibrationPowerCapability>
}) {
  const Icon = capability.ok ? CircleHelp : AlertTriangle
  const buttonClassName = capability.ok
    ? 'industrial-calibration-capability-hint'
    : 'industrial-calibration-capability-hint industrial-calibration-capability-hint--warning'

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            className={buttonClassName}
            aria-label={capability.ok ? '查看电源能力说明' : '查看电源能力告警'}
          >
            <Icon size={14} aria-hidden="true" />
          </button>
        </TooltipTrigger>
        <TooltipContent
          className="industrial-calibration-capability-tooltip"
          side="bottom"
          align="start"
        >
          <strong>{capability.summary}</strong>
          {capability.ok ? (
            <p>
              {capability.currentProxyMa != null
                ? `按当前电源能力工作。电流代理值 ${formatAmps(capability.currentProxyMa)} 只在 CC 环路下用于评估加热板温度与电阻曲线。`
                : '按当前电源能力工作。电流代理值只在 CC 环路下用于评估加热板温度与电阻曲线。'}
            </p>
          ) : (
            <ul>
              {capability.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          )}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}

function PropertyList({ items }: { items: Array<[label: string, value: string]> }) {
  return (
    <dl className="industrial-calibration-property-list">
      {items.map(([label, value]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  )
}

function PpsCalibrationFields({
  voltageText,
  onVoltageChange,
  range,
  disabled,
}: {
  voltageText: string
  onVoltageChange: (value: string) => void
  range: ReturnType<typeof ppsCapabilityRange>
  disabled: boolean
}) {
  const minVoltageMv = range?.minMv ?? PPS_HARDWARE_MIN_MV
  const maxVoltageMv = range?.maxMv ?? PPS_HARDWARE_MAX_MV
  const voltageSliderValue = Number.isFinite(Number(voltageText))
    ? Math.min(Math.max(Number(voltageText), minVoltageMv), maxVoltageMv)
    : minVoltageMv

  return (
    <div className="industrial-calibration-field industrial-calibration-slider-field">
      <div className="industrial-calibration-slider-field__header">
        <span>PPS 电压</span>
        <span className="industrial-calibration-input industrial-calibration-input--compact">
          <input
            type="number"
            inputMode="numeric"
            step={PPS_STEP_MV}
            min={minVoltageMv}
            max={maxVoltageMv}
            value={voltageText}
            disabled={disabled}
            aria-label="PPS 电压输入"
            onChange={(event) => onVoltageChange(event.currentTarget.value)}
          />
          <small>mV</small>
        </span>
      </div>
      <input
        type="range"
        min={minVoltageMv}
        max={maxVoltageMv}
        step={PPS_STEP_MV}
        value={voltageSliderValue}
        disabled={disabled}
        aria-label="PPS 电压滑块"
        className="industrial-calibration-slider"
        onChange={(event) => onVoltageChange(event.currentTarget.value)}
      />
      <div className="industrial-calibration-slider-field__bounds">
        <span>{range ? formatVolts(range.minMv) : '无 PPS 能力'}</span>
        <span>{range ? formatVolts(range.maxMv) : '不可用'}</span>
      </div>
    </div>
  )
}

function AdcCalibrationToolbar({
  disabled = false,
  feedback,
  onExport,
  onImport,
}: {
  disabled?: boolean
  feedback: ActionFeedback
  onExport: () => void
  onImport: () => void
}) {
  return (
    <section className="industrial-calibration-adc-toolbar" aria-label="ADC 标定操作">
      <div className="industrial-calibration-command-bar">
        <button
          type="button"
          className="industrial-button industrial-button--secondary industrial-calibration-command-bar__action"
          onClick={onExport}
        >
          <Download size={15} aria-hidden="true" />
          导出
        </button>
        <button
          type="button"
          className="industrial-button industrial-button--secondary industrial-calibration-command-bar__action"
          disabled={disabled}
          onClick={onImport}
        >
          <Upload size={15} aria-hidden="true" />
          导入
        </button>
      </div>
      <ActionFeedbackPanel feedback={feedback} compact />
    </section>
  )
}

function HeaterCurvePanel({
  device,
  heaterCurve,
  draftText,
  disabled = false,
  onDraftTextChange,
}: {
  device: DeviceTarget
  heaterCurve: HeaterCurveState
  draftText: string
  disabled?: boolean
  onDraftTextChange: (value: string) => void
}) {
  const heaterCurveEditorRef = useRef<HTMLTextAreaElement | null>(null)
  const heaterCurveTableColumns = heaterCurve.preview
    ? '3rem minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1fr)'
    : '3rem minmax(0, 1fr) minmax(0, 1fr)'
  const activeRows = heaterCurve.active.points.map((point, index) => ({
    index,
    point,
    preview: heaterCurve.preview?.points[index] ?? null,
  }))

  useLayoutEffect(() => {
    const editor = heaterCurveEditorRef.current
    if (!editor) {
      return
    }

    editor.style.height = '0px'
    editor.style.height = `${editor.scrollHeight}px`
  }, [])

  return (
    <section className="industrial-heater-curve-panel" aria-label="加热曲线">
      <div className="industrial-heater-curve-panel__header">
        <div>
          <h3 className="industrial-section-title">曲线数据</h3>
          <p className="industrial-heater-curve-panel__subtitle">
            预览与 EEPROM 数据统一在这里对照查看。
          </p>
        </div>
      </div>

      <section className="industrial-heater-curve-table-wrap" aria-label="加热曲线点表">
        <table
          className="industrial-heater-curve-table"
          aria-label="加热曲线点表"
          style={
            {
              '--industrial-heater-curve-table-columns': heaterCurveTableColumns,
            } as CSSProperties
          }
        >
          <thead>
            <tr>
              <th scope="col">槽位</th>
              <th scope="col">当前温度</th>
              <th scope="col">当前电阻</th>
              {heaterCurve.preview ? <th scope="col">预览温度</th> : null}
              {heaterCurve.preview ? <th scope="col">预览电阻</th> : null}
            </tr>
          </thead>
          <tbody>
            {activeRows.map(({ index, point, preview }) => (
              <tr key={index}>
                <td>#{index + 1}</td>
                <td>{point ? formatHeaterCurveTemp(point.tempCentiC) : '—'}</td>
                <td>{point ? formatHeaterCurveResistance(point.resistanceMilliohms) : '—'}</td>
                {heaterCurve.preview ? (
                  <>
                    <td>{preview ? formatHeaterCurveTemp(preview.tempCentiC) : '—'}</td>
                    <td>
                      {preview ? formatHeaterCurveResistance(preview.resistanceMilliohms) : '—'}
                    </td>
                  </>
                ) : null}
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <details className="industrial-heater-curve-editor">
        <summary>JSON 编辑器</summary>
        <label
          className="industrial-heater-curve-editor__label"
          htmlFor={`heater-curve-json-${device.id}`}
        >
          加热曲线 JSON
        </label>
        <Textarea
          id={`heater-curve-json-${device.id}`}
          ref={heaterCurveEditorRef}
          className="industrial-heater-curve-editor__textarea"
          disabled={disabled}
          value={draftText}
          onChange={(event) => onDraftTextChange(event.currentTarget.value)}
        />
      </details>
    </section>
  )
}

function HeaterCurveWorkbenchCard({
  device,
  heaterCurve,
  draftText,
  disabled = false,
  currentModeError,
  currentJobMessage,
  runtimeCalibration,
  onDraftTextChange,
  onPreview,
  onClearPreview,
  onSave,
}: {
  device: DeviceTarget
  heaterCurve: HeaterCurveState
  draftText: string
  disabled?: boolean
  currentModeError?: string | null
  currentJobMessage?: string | null
  runtimeCalibration: CalibrationRuntimeState
  onDraftTextChange: (value: string) => void
  onPreview: (heaterCurve: HeaterCurvePackage) => void | Promise<void>
  onClearPreview: () => void | Promise<void>
  onSave: () => void | Promise<void>
}) {
  const previewCount = heaterCurve.preview ? countHeaterCurvePoints(heaterCurve.preview) : 0

  const parseDraft = () => {
    const parsed = JSON.parse(draftText) as HeaterCurvePackage | { package?: HeaterCurvePackage }
    const packageValue = 'points' in parsed ? parsed : parsed.package
    if (!packageValue) {
      throw new Error('加热曲线 JSON 必须包含 points 数组。')
    }
    return normalizeHeaterCurvePackage(packageValue)
  }

  return (
    <CalibrationWorkbenchCard
      title="状态"
      summary={
        <PropertyList
          items={[
            ['目标温度', formatTemp(device.targetTempC)],
            ['加热', runtimeCalibration.heaterEnabled ? '开启' : '关闭'],
            ['预览', heaterCurve.preview ? `${previewCount}/8` : '无'],
          ]}
        />
      }
      guidance={
        currentModeError == null && currentJobMessage == null
          ? '预览立即生效；保存后才会写入 EEPROM。'
          : null
      }
      messages={
        <>
          {currentModeError ? (
            <p className="industrial-calibration-inline-error">{currentModeError}</p>
          ) : null}
          {currentJobMessage ? (
            <p className="industrial-calibration-inline-error">{currentJobMessage}</p>
          ) : null}
        </>
      }
    >
      <div className="industrial-heater-curve-toolbar">
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
          disabled={disabled}
          onClick={() => {
            try {
              onDraftTextChange(JSON.stringify(heaterCurve.preview ?? heaterCurve.active, null, 2))
            } catch {
              onDraftTextChange(JSON.stringify(heaterCurve.active, null, 2))
            }
          }}
        >
          读取曲线
        </button>
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
          disabled={disabled}
          onClick={() => {
            try {
              void onPreview(parseDraft())
            } catch {
              onDraftTextChange(JSON.stringify(heaterCurve.preview ?? heaterCurve.active, null, 2))
            }
          }}
        >
          导入预览
        </button>
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
          disabled={disabled || !heaterCurve.preview}
          onClick={() => void onClearPreview()}
        >
          清除预览
        </button>
        <button
          type="button"
          className="industrial-button industrial-button--primary"
          disabled={disabled || !heaterCurve.preview}
          onClick={() => void onSave()}
        >
          保存曲线
        </button>
      </div>
    </CalibrationWorkbenchCard>
  )
}

function CalibrationChannelControls({
  referenceLabel,
  referenceValue,
  referenceUnit,
  referenceAsPlaceholder = false,
  disabled = false,
  onReferenceChange,
  onCapture,
}: {
  referenceLabel: string
  referenceValue: number
  referenceUnit: string
  referenceAsPlaceholder?: boolean
  disabled?: boolean
  onReferenceChange: (value: number) => void
  onCapture: (referenceValue: number) => void | Promise<void>
}) {
  const [referenceText, setReferenceText] = useState(() =>
    referenceAsPlaceholder || !Number.isFinite(referenceValue) ? '' : String(referenceValue)
  )

  useEffect(() => {
    if (!referenceAsPlaceholder) {
      setReferenceText(Number.isFinite(referenceValue) ? String(referenceValue) : '')
    }
  }, [referenceAsPlaceholder, referenceValue])

  const parsedReferenceValue = Number(referenceText)
  const referenceInvalid = referenceText.trim() === '' || !Number.isFinite(parsedReferenceValue)
  const referencePlaceholder = Number.isFinite(referenceValue) ? String(referenceValue) : undefined

  return (
    <div className="industrial-calibration-sample-control-row industrial-calibration-sample-control-row--capture-only">
      <div className="industrial-calibration-capture-row">
        <label>
          <span>{referenceLabel}</span>
          <span className="industrial-calibration-input">
            <input
              type="number"
              aria-label={referenceLabel}
              disabled={disabled}
              value={referenceText}
              placeholder={referencePlaceholder}
              onChange={(event) => {
                const nextValue = event.currentTarget.value
                setReferenceText(nextValue)
                const parsedNextValue = Number(nextValue)
                if (nextValue.trim() !== '' && Number.isFinite(parsedNextValue)) {
                  onReferenceChange(parsedNextValue)
                }
              }}
            />
            <small>{referenceUnit}</small>
          </span>
        </label>
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
          disabled={disabled || referenceInvalid}
          onClick={() => onCapture(parsedReferenceValue)}
        >
          采集样本
        </button>
      </div>
    </div>
  )
}

function CalibrationFittedSuggestionCard({ title, fit }: { title: string; fit: CalibrationFit }) {
  return (
    <fieldset className="industrial-calibration-manual-fit" aria-label={`${title} 拟合建议`}>
      <div className="industrial-calibration-fit-readout">
        <span>拟合增益</span>
        <strong>
          {fit.gain.toFixed(5)}
          <small>x</small>
        </strong>
      </div>
      <div className="industrial-calibration-fit-readout">
        <span>拟合偏移</span>
        <strong>
          {fit.offsetMv.toFixed(1)}
          <small>mV</small>
        </strong>
      </div>
      <div className="industrial-calibration-fit-suggestion-meta">
        <span className="industrial-calibration-fit-chip">{calibrationFitMode(fit)}</span>
        <small>{fit.sampleCount}/8</small>
      </div>
    </fieldset>
  )
}

function CalibrationSlotEditOverlay({
  slotEditor,
  fittedFit,
  onChange,
  onAdoptFit,
  onClose,
  onSubmit,
}: {
  slotEditor: {
    channel: CalibrationChannel
    slot: CalibrationSlotId
    gainText: string
    offsetText: string
  } | null
  fittedFit: CalibrationFit | null
  onChange: (next: { gainText?: string; offsetText?: string }) => void
  onAdoptFit: () => void
  onClose: () => void
  onSubmit: () => void | Promise<void>
}) {
  const titleId = useId()
  const gainInputId = useId()
  const offsetInputId = useId()

  if (!slotEditor) {
    return null
  }

  return createPortal(
    <div
      className="industrial-slot-editor-overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
    >
      <button
        type="button"
        className="industrial-slot-editor-backdrop"
        aria-label="关闭槽位编辑"
        onClick={onClose}
      />
      <Card className="industrial-slot-editor-card" role="document">
        <CardHeader className="industrial-slot-editor-card__header">
          <div>
            <span className="industrial-slot-editor-card__eyebrow">校准槽位</span>
            <CardTitle className="industrial-slot-editor-card__title" id={titleId}>
              {`${channelLabel(slotEditor.channel)} 槽位 ${slotEditor.slot.toUpperCase()}`}
            </CardTitle>
          </div>
          <span className="industrial-slot-editor-card__slot">{slotEditor.slot.toUpperCase()}</span>
        </CardHeader>
        <CardContent className="industrial-slot-editor-card__body">
          <CalibrationSlotEditorField
            id={gainInputId}
            label="增益"
            unit="x"
            value={slotEditor.gainText}
            onValueChange={(gainText) => onChange({ gainText })}
          />
          <CalibrationSlotEditorField
            id={offsetInputId}
            label="偏移"
            unit="mV"
            value={slotEditor.offsetText}
            onValueChange={(offsetText) => onChange({ offsetText })}
          />
        </CardContent>
        <CardFooter className="industrial-slot-editor-card__actions">
          <div className="industrial-slot-editor-card__actions-left">
            {fittedFit ? (
              <Button
                type="button"
                variant="secondary"
                size="sm"
                className="industrial-slot-editor-button industrial-slot-editor-button--fit"
                onClick={onAdoptFit}
              >
                采用拟合
              </Button>
            ) : null}
          </div>
          <div className="industrial-slot-editor-card__actions-right">
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="industrial-slot-editor-button industrial-slot-editor-button--cancel"
              onClick={onClose}
            >
              取消
            </Button>
            <Button
              type="button"
              size="sm"
              className="industrial-slot-editor-button industrial-slot-editor-button--save"
              onClick={() => void onSubmit()}
            >
              保存
            </Button>
          </div>
        </CardFooter>
      </Card>
    </div>,
    document.body
  )
}

function CalibrationSlotEditorField({
  id,
  label,
  unit,
  value,
  onValueChange,
}: {
  id: string
  label: string
  unit: string
  value: string
  onValueChange: (value: string) => void
}) {
  return (
    <div className="industrial-slot-editor-field">
      <Label className="industrial-slot-editor-label" htmlFor={id}>
        {label}
      </Label>
      <div className="industrial-slot-editor-control">
        <Input
          id={id}
          type="number"
          inputMode="decimal"
          aria-label={label}
          value={value}
          className="industrial-slot-editor-text-input"
          onChange={(event) => onValueChange(event.currentTarget.value)}
        />
        <span className="industrial-slot-editor-unit" aria-hidden="true">
          {unit}
        </span>
      </div>
    </div>
  )
}

function CalibrationFitStatusSummary({
  liveLabel,
  liveValue,
  channelState,
  channel,
  disabled = false,
  onSetActiveSlot,
  onEditSlot,
}: {
  liveLabel: string
  liveValue: string
  channelState: CalibrationChannelState
  channel: CalibrationChannel
  disabled?: boolean
  onSetActiveSlot: (channel: CalibrationChannel, slot: CalibrationSlotId) => void | Promise<void>
  onEditSlot: (
    channel: CalibrationChannel,
    slot: CalibrationSlotId,
    fit: CalibrationSlotFit
  ) => void
}) {
  const renderSlotRow = (slot: CalibrationSlotId, fit: CalibrationSlotFit) => (
    <div className="industrial-calibration-property-list__fit-group" key={slot}>
      <dt>{`槽位 ${slot.toUpperCase()}`}</dt>
      <dd>
        <button
          type="button"
          className={cn(
            'industrial-calibration-fit-chip',
            channelState.activeSlot === slot && 'is-active'
          )}
          disabled={disabled}
          onClick={() => void onSetActiveSlot(channel, slot)}
        >
          {channelState.activeSlot === slot ? '激活中' : '切换'}
        </button>
        <span>{fit.gain.toFixed(5)}x</span>
        <span>{fit.offsetMv.toFixed(1)}mV</span>
        <button
          type="button"
          className="industrial-button industrial-button--ghost industrial-calibration-slot-edit"
          disabled={disabled}
          onClick={() => onEditSlot(channel, slot, fit)}
        >
          编辑
        </button>
      </dd>
    </div>
  )

  return (
    <dl
      className="industrial-calibration-property-list industrial-calibration-property-list--fit-card"
      aria-label={`${liveLabel} 标定状态摘要`}
    >
      <div className="industrial-calibration-property-list__fit-group">
        <dt>{liveLabel}</dt>
        <dd>
          <span>{liveValue}</span>
        </dd>
      </div>
      {renderSlotRow('a', channelState.slots.a)}
      {renderSlotRow('b', channelState.slots.b)}
    </dl>
  )
}

function CalibrationHeaterFeedback({
  heaterEnabled,
  heaterOutputPercent,
}: {
  heaterEnabled: boolean
  heaterOutputPercent: number
}) {
  const clampedOutput = Math.max(0, Math.min(heaterOutputPercent, 100))
  return (
    <div className="industrial-calibration-heater-feedback">
      <div className="industrial-calibration-heater-feedback__header">
        <span>{heaterEnabled ? '加热强度' : '加热已停'}</span>
        <strong>{clampedOutput}%</strong>
      </div>
      <meter
        className="industrial-heat-output industrial-calibration-heater-feedback__meter"
        aria-label="加热强度"
        value={clampedOutput}
        min={0}
        max={100}
      >
        <span style={{ width: `${clampedOutput}%` }} />
      </meter>
    </div>
  )
}

function CalibrationWorkbenchCard({
  title = '状态',
  summary,
  guidance,
  messages,
  children,
}: {
  title?: string
  summary: ReactNode
  guidance?: ReactNode
  messages?: ReactNode
  children?: ReactNode
}) {
  return (
    <CalibrationLiveCard title={title} compact>
      {summary}
      {guidance ? <div className="industrial-calibration-guidance">{guidance}</div> : null}
      {children ? <div className="industrial-calibration-work-body">{children}</div> : null}
      {messages ? <div className="industrial-calibration-work-messages">{messages}</div> : null}
    </CalibrationLiveCard>
  )
}

function CalibrationChannelSamples({
  channel,
  title,
  guidance,
  controls,
  messages,
  samples,
  disabled = false,
  onDelete,
}: {
  channel: CalibrationChannel
  title: string
  guidance?: ReactNode
  controls?: ReactNode
  messages?: ReactNode
  samples: Array<RtdCalibrationSample | VinCalibrationSample | null>
  disabled?: boolean
  onDelete: (sampleIndex: number) => void | Promise<void>
}) {
  const sampleCount = samples.filter((sample) => isValidCalibrationSample(sample, channel)).length
  const sampleKeys = calibrationSampleKeys(samples)
  const isRtdChannel = channel === 'rtd_adc'
  const populatedSamples = samples
    .map((sample, index) =>
      isValidCalibrationSample(sample, channel) ? { ...sample, index } : null
    )
    .filter(
      (
        sample
      ): sample is (RtdCalibrationSample | VinCalibrationSample) & {
        index: number
      } => Boolean(sample)
    )
  const rtdSamplePairs = isRtdChannel
    ? populatedSamples.reduce<Array<Array<RtdCalibrationSample & { index: number }>>>(
        (rows, sample) => {
          if (!isRtdCalibrationSample(sample)) {
            return rows
          }
          const currentRow = rows[rows.length - 1]
          if (!currentRow || currentRow.length === 2) {
            rows.push([sample])
          } else {
            currentRow.push(sample)
          }
          return rows
        },
        []
      )
    : []
  const vinSamplePairs = !isRtdChannel
    ? populatedSamples.reduce<Array<Array<VinCalibrationSample & { index: number }>>>(
        (rows, sample) => {
          if (!isVinCalibrationSample(sample)) {
            return rows
          }
          const currentRow = rows[rows.length - 1]
          if (!currentRow || currentRow.length === 2) {
            rows.push([sample])
          } else {
            currentRow.push(sample)
          }
          return rows
        },
        []
      )
    : []

  return (
    <section className="industrial-calibration-samples-scroll" aria-label={`${title} 样本列表`}>
      <div className="industrial-calibration-channel__header">
        <h3 className="industrial-section-title">{title}</h3>
        <span>{sampleCount}/8 个样本</span>
      </div>
      {guidance ? <div className="industrial-calibration-guidance">{guidance}</div> : null}
      {controls ? <div className="industrial-calibration-sample-controls">{controls}</div> : null}
      {messages ? <div className="industrial-calibration-work-messages">{messages}</div> : null}
      {isRtdChannel ? (
        <table
          className={cn(
            'industrial-calibration-samples industrial-calibration-samples--paired',
            populatedSamples.length === 0 && 'industrial-calibration-samples--empty'
          )}
          aria-label={`${title} 样本`}
        >
          <thead>
            <tr>
              <th scope="col">ADC 电压</th>
              <th scope="col">温度</th>
              <th scope="col">操作</th>
              <th scope="col">ADC 电压</th>
              <th scope="col">温度</th>
              <th scope="col">操作</th>
            </tr>
          </thead>
          <tbody>
            {populatedSamples.length > 0 ? (
              rtdSamplePairs.map((pair, pairIndex) => (
                <tr
                  key={
                    pair.map((sample) => sampleKeys[sample.index]).join('-') || `rtd-${pairIndex}`
                  }
                >
                  {pair.map((sample) => (
                    <Fragment key={sampleKeys[sample.index]}>
                      <td>
                        <strong>{formatRtdCalibrationTargetAdc(sample)}</strong>
                      </td>
                      <td>
                        <strong>{formatRtdCalibrationReference(sample)}</strong>
                      </td>
                      <td>
                        <button
                          type="button"
                          className="industrial-button industrial-button--danger-quiet"
                          disabled={disabled}
                          aria-label={`删除 ${title} 样本 ${sample.index + 1}`}
                          onClick={() => onDelete(sample.index)}
                        >
                          <Trash2 size={14} aria-hidden="true" />
                          删除
                        </button>
                      </td>
                    </Fragment>
                  ))}
                  {pair.length === 1 ? (
                    <>
                      <td aria-hidden="true" />
                      <td aria-hidden="true" />
                      <td aria-hidden="true" />
                    </>
                  ) : null}
                </tr>
              ))
            ) : (
              <tr className="industrial-calibration-samples__placeholder-row">
                <td>—</td>
                <td>—</td>
                <td>—</td>
                <td>—</td>
                <td>—</td>
                <td>—</td>
              </tr>
            )}
          </tbody>
        </table>
      ) : (
        <table
          className={cn(
            'industrial-calibration-samples industrial-calibration-samples--paired',
            populatedSamples.length === 0 && 'industrial-calibration-samples--empty'
          )}
          aria-label={`${title} 样本`}
        >
          <thead>
            <tr>
              <th scope="col">ADC 电压</th>
              <th scope="col">参考电压</th>
              <th scope="col">操作</th>
              <th scope="col">ADC 电压</th>
              <th scope="col">参考电压</th>
              <th scope="col">操作</th>
            </tr>
          </thead>
          <tbody>
            {populatedSamples.length > 0 ? (
              vinSamplePairs.map((pair, pairIndex) => (
                <tr
                  key={
                    pair.map((sample) => sampleKeys[sample.index]).join('-') || `vin-${pairIndex}`
                  }
                >
                  {pair.map((sample) => (
                    <Fragment key={sampleKeys[sample.index]}>
                      <td>
                        <strong>{sample.observedMv}mV</strong>
                      </td>
                      <td>
                        <strong>{sample.expectedMv}mV</strong>
                      </td>
                      <td>
                        <button
                          type="button"
                          className="industrial-button industrial-button--danger-quiet"
                          disabled={disabled}
                          aria-label={`删除 ${title} 样本 ${sample.index + 1}`}
                          onClick={() => onDelete(sample.index)}
                        >
                          <Trash2 size={14} aria-hidden="true" />
                          删除
                        </button>
                      </td>
                    </Fragment>
                  ))}
                  {pair.length === 1 ? (
                    <>
                      <td aria-hidden="true" />
                      <td aria-hidden="true" />
                      <td aria-hidden="true" />
                    </>
                  ) : null}
                </tr>
              ))
            ) : (
              <tr className="industrial-calibration-samples__placeholder-row">
                <td>—</td>
                <td>—</td>
                <td>—</td>
                <td>—</td>
                <td>—</td>
                <td>—</td>
              </tr>
            )}
          </tbody>
        </table>
      )}
    </section>
  )
}

function formatHeaterCurveTemp(value: number) {
  return `${(value / 100).toFixed(2).replace(/\.00$/, '')}℃`
}

function formatHeaterCurveResistance(value: number) {
  return `${(value / 1000).toFixed(3).replace(/0+$/, '').replace(/\.$/, '')}Ω`
}

function countHeaterCurvePoints(packageValue: HeaterCurvePackage) {
  return packageValue.points.filter(Boolean).length
}

function createDefaultHeaterCurveState(): HeaterCurveState {
  const empty = createEmptyHeaterCurvePackage()
  return {
    active: cloneHeaterCurvePackage(empty),
    preview: null,
  }
}

function createEmptyHeaterCurvePackage(): HeaterCurvePackage {
  return {
    points: Array.from({ length: 8 }, () => null),
  }
}

function cloneHeaterCurvePackage(packageValue: HeaterCurvePackage): HeaterCurvePackage {
  return {
    points: packageValue.points.map((point) => (point ? { ...point } : null)),
  }
}

function normalizeHeaterCurvePackage(packageValue: HeaterCurvePackage): HeaterCurvePackage {
  const points = packageValue.points
    .filter((point): point is NonNullable<typeof point> => Boolean(point))
    .map((point) => ({ ...point }))
    .sort((left, right) => left.tempCentiC - right.tempCentiC)
  return {
    points: Array.from({ length: 8 }, (_, index) => points[index] ?? null),
  }
}

function applyLocalHeaterCurveRequest(
  current: HeaterCurveState,
  request: Omit<HeaterCurveConfigRequest, 'leaseId'>
): HeaterCurveState {
  if (request.op === 'preview') {
    if (!request.package) {
      throw new Error('缺少加热曲线数据包。')
    }
    return {
      ...current,
      preview: normalizeHeaterCurvePackage(request.package),
    }
  }

  return {
    ...current,
    preview: null,
  }
}

function applyLocalHeaterCurveSave(current: HeaterCurveState): HeaterCurveState {
  if (!current.preview) {
    throw new Error('保存前必须先存在预览曲线。')
  }
  return {
    active: cloneHeaterCurvePackage(current.preview),
    preview: null,
  }
}

function PresetTemperatureEditor({
  selectedPresetIndex,
  presetTemps,
  presetEnabled,
  onPresetSlotChange,
  onPresetTempChange,
  onPresetEnabledChange,
}: {
  selectedPresetIndex: number
  presetTemps: number[]
  presetEnabled: boolean[]
  onPresetSlotChange: (presetIndex: number) => void | Promise<void>
  onPresetTempChange: (nextTempC: number) => void | Promise<void>
  onPresetEnabledChange: (nextEnabled: boolean) => void | Promise<void>
}) {
  const selectedTemp = presetTemps[selectedPresetIndex] ?? PRESET_TEMPS_C[selectedPresetIndex]
  const selectedEnabled = presetEnabled[selectedPresetIndex] ?? true
  const [draftTemp, setDraftTemp] = useState(selectedTemp)
  const draftIsDirty = selectedEnabled && clampTargetTemp(draftTemp) !== selectedTemp

  useEffect(() => {
    setDraftTemp(selectedTemp)
  }, [selectedTemp])

  useEffect(() => {
    const clampedDraftTemp = clampTargetTemp(draftTemp)

    if (!draftIsDirty) {
      return
    }

    const timer = window.setTimeout(() => {
      void onPresetTempChange(clampedDraftTemp)
    }, PRESET_COMMIT_DEBOUNCE_MS)

    return () => window.clearTimeout(timer)
  }, [draftIsDirty, draftTemp, onPresetTempChange])

  const handleDraftTempChange = (nextTempC: number) => {
    setDraftTemp(clampTargetTemp(nextTempC))
  }

  return (
    <div className="industrial-preset-editor">
      <div className="industrial-preset-slots">
        {PRESET_SLOT_IDS.map((slotId, index) => {
          const tempC = presetTemps[index] ?? PRESET_TEMPS_C[index]
          const isEnabled = presetEnabled[index] ?? true
          const isSelected = index === selectedPresetIndex

          return (
            <button
              key={slotId}
              type="button"
              className={[isSelected ? 'is-selected' : '', isEnabled ? '' : 'is-disabled'].join(
                ' '
              )}
              aria-pressed={isSelected}
              aria-label={`${slotId} ${formatPresetTemp(tempC, isEnabled)} ${isEnabled ? 'enabled' : 'disabled'}`}
              onClick={() => void onPresetSlotChange(index)}
            >
              <strong>{slotId}</strong>
              <span>{formatPresetTemp(tempC, isEnabled)}</span>
              {!isEnabled ? <small>OFF</small> : null}
            </button>
          )
        })}
      </div>

      <div className="industrial-preset-editor__control">
        <div className="industrial-preset-editor__selected">
          <p className="sr-only">Selected slot</p>
          <strong>
            M{selectedPresetIndex + 1}
            <span>{formatPresetTemp(selectedTemp, selectedEnabled)}</span>
          </strong>
          <small>{selectedEnabled ? (draftIsDirty ? 'Saving...' : 'Autosaved') : 'Disabled'}</small>
        </div>
        <TargetTempControl
          label="Preset temp"
          ariaLabel="Preset temperature"
          inputId="preset-temperature"
          inputName="presetTemperature"
          value={draftTemp}
          disabled={!selectedEnabled}
          onChange={handleDraftTempChange}
        />
        <div className="industrial-preset-switch">
          <p>
            <span className="industrial-label">Preset</span>
            <strong>{selectedEnabled ? 'Enabled' : 'Disabled'}</strong>
          </p>
          <span className="industrial-preset-switch__assembly">
            <span aria-hidden="true">OFF</span>
            <Switch
              checked={selectedEnabled}
              size="industrial"
              className="industrial-preset-switch__control"
              aria-label={`Preset M${selectedPresetIndex + 1}`}
              onCheckedChange={(checked) => void onPresetEnabledChange(checked)}
            />
            <span aria-hidden="true">ON</span>
          </span>
        </div>
      </div>
    </div>
  )
}

function UpdateView({
  device,
  artifacts,
  artifact,
  flashPhases,
  feedback,
  flashRun,
  onArtifactChange,
  onStartDryRun,
  onStartFlash,
}: {
  device: DeviceTarget
  artifacts: FirmwareArtifact[]
  artifact?: FirmwareArtifact
  flashPhases: WorkflowPhase[]
  feedback: ActionFeedback
  flashRun: { status: FlashRunStatus; progress: number }
  onArtifactChange: (artifactId: string) => void
  onStartDryRun: () => void
  onStartFlash: () => void
}) {
  const blockedPhase = flashPhases.find((phase) => phase.state === 'blocked')
  const activePhase = flashPhases.find((phase) => phase.state === 'active') ?? flashPhases[0]
  const currentProgress =
    flashRun.status === 'idle' ? (artifact?.progressPercent ?? 0) : flashRun.progress
  const isBlocked =
    device.severity === 'offline' ||
    artifact?.compatibility === 'blocked' ||
    Boolean(blockedPhase) ||
    !device.capabilities.includes('flash') ||
    device.leaseState === 'conflict' ||
    device.leaseState === 'expired'
  const isBusy = flashRun.status === 'running' || flashRun.status === 'flashing'
  const canFlash =
    flashRun.status === 'passed' &&
    device.transport === 'devd' &&
    !isBlocked &&
    Boolean(device.leaseId)
  const verdict = isBlocked
    ? {
        tone: 'danger',
        title: 'Not compatible',
        detail:
          device.severity === 'offline'
            ? 'Target is offline.'
            : device.leaseState === 'conflict'
              ? 'Another client owns the USB lease.'
              : !device.capabilities.includes('flash')
                ? 'This transport does not expose flash capability.'
                : (blockedPhase?.detail ?? 'Selected firmware does not match this target.'),
      }
    : flashRun.status === 'flashed'
      ? {
          tone: 'safe',
          title: 'Flash complete',
          detail: `${artifact?.version ?? 'Artifact'} was written by devd.`,
        }
      : flashRun.status === 'flashing'
        ? {
            tone: 'warning',
            title: 'Writing firmware',
            detail: `${artifact?.version ?? 'Artifact'} is being written by devd.`,
          }
        : flashRun.status === 'passed'
          ? {
              tone: 'safe',
              title: 'Check passed',
              detail: `${artifact?.version ?? 'Artifact'} is verified and ready for guarded flash.`,
            }
          : artifact?.compatibility === 'warning'
            ? {
                tone: 'warning',
                title: 'Check recommended',
                detail: `${artifact.version} can be checked, but the profile differs from the active runtime.`,
              }
            : {
                tone: 'safe',
                title: 'Ready to check',
                detail: `${activePhase?.label ?? 'Dry-run'} can run without changing firmware.`,
              }

  const recoveryNote =
    deviceControlBlockReason(device) && !isBlocked
      ? 'Serial control is degraded; firmware recovery remains available through devd flash.'
      : null

  return (
    <div className="industrial-view-panel">
      <PanelHeader kicker="Update" title="Firmware check" />
      <div className={`industrial-gate-verdict is-${verdict.tone}`}>
        <div>
          <p className="industrial-label">Compatibility</p>
          <strong>{verdict.title}</strong>
          <span>{verdict.detail}</span>
        </div>
        <CheckCircle2 size={22} aria-hidden="true" />
      </div>
      <div className="industrial-update-grid">
        <div className="industrial-artifact industrial-artifact--compact">
          <p className="industrial-label">Artifact</p>
          <Select value={artifact?.id} onValueChange={onArtifactChange}>
            <SelectTrigger
              aria-label="Firmware artifact"
              className="industrial-artifact-select industrial-radix-select"
              disabled={isBusy || artifacts.length === 0}
            >
              <SelectValue placeholder="No firmware artifact" />
            </SelectTrigger>
            <SelectContent className="industrial-select-content">
              {artifacts.map((item) => (
                <SelectItem key={item.id} value={item.id} className="industrial-select-item">
                  {item.version} · {item.profile}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <p className="industrial-mono text-xs">
            {artifact?.target ?? 'target unknown'} · {artifact?.protocol ?? 'protocol unknown'} ·{' '}
            {artifact?.hash ?? 'hash unavailable'}
          </p>
          <div
            className="industrial-progress"
            role="progressbar"
            aria-label={`${artifact?.id ?? 'artifact'} flash progress`}
            aria-valuenow={currentProgress}
            aria-valuemin={0}
            aria-valuemax={100}
          >
            <span
              style={{
                width: `${currentProgress}%`,
              }}
            />
          </div>
        </div>
        <CompactPhase label="Dry-run" phases={flashPhases} />
      </div>
      {recoveryNote ? <p className="industrial-mono text-xs">{recoveryNote}</p> : null}
      <div className="industrial-command-row">
        <button
          type="button"
          className="industrial-button industrial-button--primary"
          disabled={isBusy || isBlocked}
          onClick={onStartDryRun}
        >
          <Upload size={16} aria-hidden="true" />
          {flashRun.status === 'running' ? 'Checking' : 'Run dry-check'}
        </button>
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
          disabled={!canFlash || isBusy}
          onClick={onStartFlash}
        >
          <Zap size={16} aria-hidden="true" />
          {flashRun.status === 'flashing'
            ? 'Flashing'
            : flashRun.status === 'flashed'
              ? 'Flashed'
              : 'Flash'}
        </button>
      </div>
      <ActionFeedbackPanel feedback={feedback} />
    </div>
  )
}

function GlobalLogPanel({ events }: { events: EventLogEntry[] }) {
  const scrollableNodeRef = useRef<HTMLDivElement | null>(null)
  const [followTail, setFollowTail] = useState(false)
  const [logFilter, setLogFilter] = useState<LogFilter>('all')
  const filteredEvents = useMemo(
    () => (logFilter === 'all' ? events : events.filter((event) => event.tone === logFilter)),
    [events, logFilter]
  )
  const rowVirtualizer = useVirtualizer({
    count: filteredEvents.length,
    getScrollElement: () => scrollableNodeRef.current,
    estimateSize: () => 112,
    measureElement: (element) => element.getBoundingClientRect().height,
    overscan: 8,
  })

  useLayoutEffect(() => {
    if (followTail && filteredEvents.length > 0) {
      rowVirtualizer.scrollToIndex(filteredEvents.length - 1, { align: 'end' })
    }
  }, [filteredEvents.length, followTail, rowVirtualizer])

  const handleLogScroll = () => {
    const scrollElement = scrollableNodeRef.current

    if (!scrollElement || !followTail) {
      return
    }

    const distanceFromTail =
      scrollElement.scrollHeight - scrollElement.scrollTop - scrollElement.clientHeight

    if (distanceFromTail > 96) {
      setFollowTail(false)
    }
  }

  const handleFollowTailToggle = () => {
    setFollowTail((current) => {
      const next = !current

      if (next) {
        window.requestAnimationFrame(() => {
          if (filteredEvents.length > 0) {
            rowVirtualizer.scrollToIndex(filteredEvents.length - 1, {
              align: 'end',
            })
          }
        })
      }

      return next
    })
  }

  const virtualItems = rowVirtualizer.getVirtualItems()
  const latestEvent = filteredEvents.at(-1)
  const latestSourceLabel = latestEvent?.source
    ? (eventSourceLabels[latestEvent.source] ?? latestEvent.source.toUpperCase())
    : '追踪'

  return (
    <aside className="industrial-panel industrial-log-panel" aria-label="全局日志">
      <div className="industrial-log-panel__header">
        <div>
          <p className="industrial-label text-[#a8b2d1]">全局日志</p>
          <h2>运行时追踪</h2>
        </div>
        <fieldset className="industrial-log-filters">
          <legend className="sr-only">日志级别筛选</legend>
          {LOG_FILTER_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              className={option.value === logFilter ? 'is-selected' : ''}
              aria-pressed={option.value === logFilter}
              onClick={() => setLogFilter(option.value)}
            >
              {option.label}
            </button>
          ))}
        </fieldset>
      </div>
      <div className="industrial-log-panel__summary">
        <span>{latestEvent?.time}</span>
        <strong>{latestSourceLabel}</strong>
        <p>{latestEvent?.message ?? '暂无追踪帧'}</p>
      </div>
      <SimpleBar
        autoHide
        className="industrial-log-panel__rows"
        scrollbarMinSize={64}
        scrollableNodeProps={{
          ref: scrollableNodeRef,
          'aria-live': 'polite',
          'aria-atomic': 'false',
          onScroll: handleLogScroll,
        }}
      >
        <button
          type="button"
          className="industrial-log-follow"
          aria-pressed={followTail}
          onClick={handleFollowTailToggle}
        >
          <ToggleRight size={16} aria-hidden="true" />
          {followTail ? '跟随尾部' : '跟随尾部'}
        </button>
        <div className="industrial-log-count" aria-live="polite">
          {filteredEvents.length} / {events.length} 帧
        </div>
        <div
          className="industrial-log-virtual-space"
          style={{ height: `${rowVirtualizer.getTotalSize()}px` }}
        >
          {virtualItems.map((virtualItem) => {
            const event = filteredEvents[virtualItem.index]

            if (!event) {
              return null
            }

            return (
              <div
                key={virtualItem.key}
                ref={rowVirtualizer.measureElement}
                className={`industrial-event industrial-event--virtual is-${event.tone}`}
                data-index={virtualItem.index}
                style={{
                  transform: `translateY(${virtualItem.start}px)`,
                }}
              >
                <span>{event.time}</span>
                <strong>{eventSourceLabels[event.source] ?? event.source.toUpperCase()}</strong>
                <p>
                  {event.message}
                  {event.detail ? <code>{event.detail}</code> : null}
                </p>
              </div>
            )
          })}
        </div>
      </SimpleBar>
    </aside>
  )
}

function CompactPhase({ label, phases }: { label: string; phases: WorkflowPhase[] }) {
  return (
    <div className="industrial-compact-phase">
      <p className="industrial-label">{label}</p>
      <div className="industrial-phase-dots">
        {phases.slice(0, 4).map((phase) => (
          <span key={phase.label} className={`is-${phase.state}`} title={phase.label}>
            {phase.state === 'done' ? <CheckCircle2 size={14} /> : null}
          </span>
        ))}
      </div>
      <strong>{phases.find((phase) => phase.state === 'active')?.label ?? phases[0]?.label}</strong>
    </div>
  )
}

function StatusCard({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <article className="industrial-status-card">
      <p className="industrial-label">{label}</p>
      <strong>{value}</strong>
      <span>{detail}</span>
    </article>
  )
}

function SegmentedSetting({
  label,
  value,
  onChange,
  hideLabel = false,
}: {
  label: string
  value: DeviceTarget['fanState']
  onChange: (fanState: DeviceTarget['fanState']) => void
  hideLabel?: boolean
}) {
  const options: Array<Exclude<DeviceTarget['fanState'], 'RUN'>> = ['OFF', 'AUTO']

  return (
    <fieldset className="industrial-setting-control industrial-segmented-setting">
      <legend className="sr-only">{label}</legend>
      {hideLabel ? null : (
        <p className="industrial-label industrial-segmented-setting__title">{label}</p>
      )}
      <div className="industrial-segmented-control">
        {options.map((option) => (
          <button
            key={option}
            type="button"
            className={option === value ? 'is-selected' : ''}
            aria-pressed={option === value}
            onClick={() => onChange(option)}
          >
            {option}
          </button>
        ))}
      </div>
    </fieldset>
  )
}

function ActionFeedbackPanel({
  feedback,
  compact = false,
}: {
  feedback: ActionFeedback
  compact?: boolean
}) {
  return (
    <div
      className={
        compact
          ? `industrial-action-feedback industrial-action-feedback--compact is-${feedback.tone}`
          : `industrial-action-feedback is-${feedback.tone}`
      }
      aria-live="polite"
    >
      <p className="industrial-label">最近操作</p>
      <strong>{feedback.title}</strong>
      <span>{feedback.detail}</span>
    </div>
  )
}

function StatusDatum({ label, value }: { label: string; value: string }) {
  return (
    <div className="industrial-status-datum">
      <p className="industrial-label">{label}</p>
      <strong>{value}</strong>
    </div>
  )
}

function StatusPill({ severity }: { severity: DeviceSeverity }) {
  return (
    <span className={`industrial-status industrial-status--${severity}`}>
      <span className="industrial-led" aria-hidden="true" />
      {severityLabels[severity]}
    </span>
  )
}

function PanelHeader({ kicker, title }: { kicker: string; title: string }) {
  return (
    <header className="industrial-panel-header">
      <div>
        <p className="industrial-label">{kicker}</p>
        <h2>{title}</h2>
      </div>
    </header>
  )
}

export { controlPlaneScenario, degradedControlPlaneScenario }
