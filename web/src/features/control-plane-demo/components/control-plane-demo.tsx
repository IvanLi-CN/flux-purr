import { useVirtualizer } from '@tanstack/react-virtual'
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  CircleHelp,
  Download,
  Fan,
  Gauge,
  Minus,
  Plus,
  Power,
  SlidersHorizontal,
  ToggleRight,
  Trash2,
  Upload,
  Wrench,
  Zap,
} from 'lucide-react'
import type { CSSProperties, Dispatch, ReactNode, SetStateAction } from 'react'
import { Fragment, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import SimpleBar from 'simplebar-react'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import { syncCalibrationDraftText } from '../calibration-draft'
import {
  type CalibrationLeaveRequest,
  type CalibrationWorkspaceTab,
  type ConsoleView,
  shouldBlockCalibrationDeviceChange,
  shouldBlockCalibrationViewChange,
  shouldBlockCalibrationWorkspaceTabChange,
} from '../calibration-leave-guard'
import type {
  BaseCalibrationSample,
  CalibrationChannel,
  CalibrationConfigRequest,
  CalibrationControlRequest,
  CalibrationMode,
  CalibrationPackage,
  CalibrationRuntimeState,
  CalibrationState,
  HeaterCurveConfigRequest,
  HeaterCurvePackage,
  HeaterCurveState,
  RtdCalibrationSample,
  VinCalibrationSample,
} from '../contracts'
import { defaultDevdBaseUrl, type LiveDevdOptions, useLiveDevdScenario } from '../live-devd'
import {
  type LiveWebSerialControls,
  type LiveWebSerialOptions,
  useLiveWebSerialScenario,
} from '../live-web-serial'
import { controlPlaneScenario, degradedControlPlaneScenario } from '../mock-data'
import { rtdAdcMvForTemperature, rtdTemperatureForAdcMv } from '../rtd-calibration-display'
import {
  createPendingHeaterFeedback,
  deviceControlBlockReason,
  HEATER_CONFIRMATION_TIMEOUT_MS,
  heaterLockReasonText,
  type PendingHeaterConfirmation,
  resolvePendingHeaterConfirmation,
  runtimeHeaterState,
} from '../runtime-status'
import { artifactToManifest, createControlPlaneHttpClient } from '../transport-client'
import type {
  ControlPlaneScenario,
  DeviceSeverity,
  DeviceTarget,
  EventLogEntry,
  FirmwareArtifact,
  TransportKind,
  WorkflowPhase,
} from '../types'
import { isDirectWebSerialDevice } from '../web-serial'

interface ControlPlaneDemoProps {
  scenario?: ControlPlaneScenario
  initialView?: ConsoleView
  devd?: LiveDevdOptions
  webSerial?: LiveWebSerialOptions
  allowDemoControls?: boolean
}
type CalibrationWorkbenchMode = 'vin_adc' | 'rtd_adc' | 'heater_curve'
type FlashRunStatus = 'idle' | 'running' | 'passed' | 'flashing' | 'flashed'
type AddDeviceKind = 'wifi' | 'web-serial' | 'bridge'
type LogFilter = 'all' | EventLogEntry['tone']

interface ActionFeedback {
  title: string
  detail: string
  tone: 'info' | 'success' | 'warning'
}

interface CalibrationLeaveGuardState extends CalibrationLeaveRequest {
  continueAction: () => void | Promise<void>
  nextView?: ConsoleView
  nextWorkspaceTab?: CalibrationWorkspaceTab
}

const LOG_FEED_SIZE = 1000
const LOG_FEED_STEP_SECONDS = 3
const LOG_FEED_START_SECONDS = 20 * 3600 + 14 * 60 + 3
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
  id: ConsoleView
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
    boardTempC: 0,
    currentTempC: 0,
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

export function ControlPlaneDemo({
  scenario = controlPlaneScenario,
  initialView = 'dashboard',
  devd,
  webSerial: webSerialOptions,
  allowDemoControls = true,
}: ControlPlaneDemoProps) {
  const liveDevdScenario = useLiveDevdScenario(scenario, devd)
  const { scenario: liveScenario, serial: webSerial } = useLiveWebSerialScenario(
    liveDevdScenario,
    webSerialOptions
  )
  const controlClient = useMemo(
    () => devd?.httpClient ?? createControlPlaneHttpClient(),
    [devd?.httpClient]
  )
  const devdBaseUrl = devd?.devdBaseUrl ?? defaultDevdBaseUrl()
  const [selectedDeviceId, setSelectedDeviceId] = useState(scenario.selectedDeviceId)
  const [activeView, setActiveView] = useState<ConsoleView>(initialView)
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
  const [pendingDevices, setPendingDevices] = useState<DeviceTarget[]>([])
  const pendingDeviceModeRef = useRef(allowDemoControls)
  const [flashRun, setFlashRun] = useState<{ status: FlashRunStatus; progress: number }>({
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
  const [heaterConfirmationTick, setHeaterConfirmationTick] = useState(0)
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

  useEffect(() => {
    if (pendingDeviceModeRef.current === allowDemoControls) {
      return
    }

    pendingDeviceModeRef.current = allowDemoControls
    setPendingDevices([])
  }, [allowDemoControls])

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
      deviceOptions.find((device) => device.id === selectedDeviceId) ??
      deviceOptions.find((device) => device.id === activeScenario.selectedDeviceId) ??
      deviceOptions[0] ??
      activeScenario.devices[0],
    [activeScenario.devices, activeScenario.selectedDeviceId, deviceOptions, selectedDeviceId]
  )

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

    const currentSelection = deviceOptions.find((device) => device.id === selectedDeviceId)
    const shouldAdoptWebSerialTarget =
      !currentSelection ||
      isNoLiveTargetDevice(currentSelection) ||
      isPendingDeviceChoice(currentSelection)

    if (shouldAdoptWebSerialTarget && selectedDeviceId !== webSerial.deviceId) {
      setSelectedDeviceId(webSerial.deviceId)
    }
  }, [deviceOptions, selectedDeviceId, webSerial.deviceId, webSerial.state])

  useEffect(() => {
    const nextSelectedDevice = activeScenario.devices.find(
      (device) => device.id === activeScenario.selectedDeviceId
    )
    if (
      nextSelectedDevice?.transport === 'devd' &&
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

        const next = { ...current, [nextDeviceId]: clone ? clone(value) : value }
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
    migrateRecord(setCalibrationByDevice, (value) => ({
      ...value,
      active: cloneCalibrationPackage(value.active),
      draft: cloneCalibrationPackage(value.draft),
      activeFit: {
        rtdAdc: { ...value.activeFit.rtdAdc },
        vinAdc: { ...value.activeFit.vinAdc },
      },
      draftFit: {
        rtdAdc: { ...value.draftFit.rtdAdc },
        vinAdc: { ...value.draftFit.vinAdc },
      },
    }))
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
      wifiRssi: selectedDevice.wifiRssi,
      networkState: selectedDevice.networkState,
    }
  }, [
    activeScenario.devices,
    currentTempByDevice,
    fanPolicyByDevice,
    heaterHeldByDevice,
    manualPpsByDevice,
    selectedDevice,
    targetTempByDevice,
  ])
  const visibleDeviceIsLive = isLiveRuntimeDevice(visibleDevice)
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
    calibrationWorkspaceTabByDevice[visibleDevice.id] ?? 'heater_curve'
  const visibleCalibrationRefs = calibrationRefsByDevice[visibleDevice.id] ?? {
    rtdTempC: Number(visibleDevice.currentTempC.toFixed(1)),
    vinMv: visibleDevice.voltageMv,
  }

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

    setCalibrationWorkspaceTabByDevice((current) => ({
      ...current,
      [visibleDevice.id]: activeMode,
    }))
  }, [activeView, visibleCalibrationWorkspaceTab, visibleDevice.id, visibleRuntimeCalibration.mode])

  useEffect(() => {
    if (!visibleDeviceIsLive) {
      return
    }

    const blockedReason = deviceControlBlockReason(visibleDevice)
    if (!blockedReason) {
      return
    }

    const conflictTitle = visibleDevice.leaseState === 'conflict' ? '设备租约冲突' : '硬件连接受阻'
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
  }, [visibleDevice, visibleDeviceIsLive])

  useEffect(() => {
    if (!visibleDeviceIsLive || !visibleDevice.heaterLockReason) {
      return
    }

    const detail = heaterLockReasonText(visibleDevice.heaterLockReason)
    setFeedback((current) => {
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
      setHeaterConfirmationTick((current) => current + 1)
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
        detail: '当前热控状态来自 devd 固件状态。',
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
  useEffect(() => {
    if (activeView !== 'calibration') {
      return
    }
    let cancelled = false
    const load = async () => {
      try {
        if (visibleDeviceIsDirectWebSerial) {
          const calibration = await webSerial.getCalibration()
          if (!cancelled) {
            setCalibrationByDevice((current) => ({ ...current, [visibleDeviceId]: calibration }))
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
        if (!cancelled) {
          setCalibrationByDevice((current) => ({ ...current, [visibleDeviceId]: calibration }))
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
    controlClient,
    devdBaseUrl,
    visibleDeviceId,
    visibleDeviceIsDirectWebSerial,
    visibleDeviceLeaseId,
    visibleDeviceNetworkState,
    visibleDeviceTransport,
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
            setHeaterCurveByDevice((current) => ({ ...current, [visibleDeviceId]: heaterCurve }))
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
          setHeaterCurveByDevice((current) => ({ ...current, [visibleDeviceId]: heaterCurve }))
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
    () => [...actionEvents, ...scenarioEvents].slice(0, LOG_FEED_SIZE),
    [actionEvents, scenarioEvents]
  )

  const emitEvent = useCallback(
    (source: string, message: string, tone: EventLogEntry['tone'] = 'info') => {
      actionClockRef.current += LOG_FEED_STEP_SECONDS
      setActionEvents((current) =>
        [
          {
            time: formatLogTime(actionClockRef.current),
            source,
            message,
            tone,
          },
          ...current,
        ].slice(0, 24)
      )
    },
    []
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
      pendingHeaterConfirmation.requestedAtMs + heaterConfirmationTick
    )
    if (resolution.outcome === 'pending') {
      return
    }

    setPendingHeaterConfirmation(null)
    setFeedback(resolution.feedback)
    emitEvent('heater', resolution.eventMessage, resolution.eventTone)
  }, [
    emitEvent,
    heaterConfirmationTick,
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
      visibleDevice,
      webSerial,
    ]
  )

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

  const handleDeviceChange = (deviceId: string) => {
    if (deviceId === ADD_DEVICE_VALUE) {
      void requestCalibrationLeave(
        {
          reason: 'add-device-flow',
          nextLabel: '添加设备',
          nextView: 'add-device',
        },
        () => {
          setActiveView('add-device')
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
      if (webSerial.state === 'connected') {
        if (webSerial.deviceId) {
          setSelectedDeviceId(webSerial.deviceId)
          setActiveView('dashboard')
        }
        setFeedback({
          title: 'Web Serial already connected',
          detail: 'The browser Web Serial target is already listed in the target selector.',
          tone: 'info',
        })
        emitEvent('webserial', 'browser Web Serial target already connected', 'info')
        return
      }

      const connected = await handleWebSerialConnect()
      if (connected) {
        setActiveView('dashboard')
      }
      return
    }

    const nextDevice = createPendingDevice(kind)
    setPendingDevices((current) =>
      current.some((device) => device.id === nextDevice.id) ? current : [...current, nextDevice]
    )
    setSelectedDeviceId(nextDevice.id)
    setActiveView(showPendingDashboard ? 'dashboard' : 'add-device')
    setFeedback({
      title: `${nextDevice.alias} added`,
      detail:
        nextDevice.transportIssue ?? `${transportLabels[nextDevice.transport]} target pending.`,
      tone: 'warning',
    })
    emitEvent('target', `${nextDevice.alias} added from target selector`, 'warning')
  }

  const handleQuickAddDevice = async (kind: AddDeviceKind) => {
    await requestCalibrationLeave(
      {
        reason: 'add-device-flow',
        nextLabel: addDeviceOptions.find((option) => option.kind === kind)?.label ?? '添加设备',
        nextView: 'add-device',
      },
      async () => {
        setActiveView('add-device')
        await handleAddDevice(kind, { showPendingDashboard: false })
      }
    )
  }

  const handleGuardedViewChange = useCallback(
    (nextView: ConsoleView) => {
      if (nextView === activeView) {
        dismissCalibrationLeaveGuard()
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
          setActiveView(nextView)
        }
      )
    },
    [activeView, dismissCalibrationLeaveGuard, requestCalibrationLeave]
  )

  const handleGuardedWorkspaceTabChange = useCallback(
    (nextTab: CalibrationWorkspaceTab) => {
      if (nextTab === visibleCalibrationWorkspaceTab) {
        dismissCalibrationLeaveGuard()
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
          setCalibrationWorkspaceTabByDevice((current) => ({
            ...current,
            [visibleDevice.id]: nextTab,
          }))
        }
      )
    },
    [
      dismissCalibrationLeaveGuard,
      requestCalibrationLeave,
      visibleCalibrationWorkspaceTab,
      visibleDevice.id,
    ]
  )

  async function handleWebSerialConnect() {
    if (webSerial.state === 'connected') {
      await webSerial.disconnect()
      setFeedback({
        title: 'Web Serial disconnected',
        detail: 'Browser direct USB control is closed.',
        tone: 'info',
      })
      emitEvent('webserial', 'browser direct USB control disconnected', 'info')
      return false
    }

    const connected = await webSerial.connect()
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
        if (liveUpdated || targetTempCommitVersionRef.current[deviceId] !== nextVersion) {
          return
        }
        setTargetTempByDevice((current) => {
          const next = { ...current }
          delete next[deviceId]
          return next
        })
      }, 180)
    }

    setFeedback({
      title: 'Target updated',
      detail: `${visibleDevice.alias} target is now ${formatTemp(clampedTarget)}.`,
      tone: 'success',
    })
    emitEvent('thermal', `target temperature updated to ${formatTemp(clampedTarget)}`, 'success')
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
    setSelectedPresetByDevice((current) => ({ ...current, [visibleDevice.id]: presetIndex }))
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
      setPendingHeaterConfirmation({
        deviceId: visibleDevice.id,
        requestedEnabled: nextHeaterEnabled,
        requestedAtMs: Date.now(),
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

    setArtifactByDevice((current) => ({ ...current, [visibleDevice.id]: artifactId }))
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
      setCalibrationByDevice((current) => ({ ...current, [visibleDevice.id]: calibration }))
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
      setCalibrationByDevice((current) => ({ ...current, [visibleDevice.id]: calibration }))
      return
    }

    const calibration = applyLocalCalibrationRequest(visibleCalibration, request)
    setCalibrationByDevice((current) => ({ ...current, [visibleDevice.id]: calibration }))
  }

  const handleCalibrationCapture = async (
    channel: CalibrationChannel,
    options?: { targetAdcMv?: number }
  ) => {
    const request =
      channel === 'rtd_adc'
        ? {
            op: 'capture' as const,
            channel,
            referenceTempC: visibleCalibrationRefs.rtdTempC,
            targetAdcMv: options?.targetAdcMv,
          }
        : {
            op: 'capture' as const,
            channel,
            referenceVinMv: visibleCalibrationRefs.vinMv,
          }
    try {
      await updateCalibrationDraft(request)
      setFeedback({
        title: '标定草稿已更新',
        detail: `已采集 ${channelLabel(channel)} 样本。`,
        tone: 'success',
      })
      emitEvent('calibration', `captured ${channelLabel(channel)} sample`, 'success')
    } catch (error) {
      setFeedback({ title: '标定失败', detail: errorMessage(error), tone: 'warning' })
    }
  }

  const handleCalibrationDelete = async (channel: CalibrationChannel, sampleIndex: number) => {
    try {
      await updateCalibrationDraft({ op: 'delete', channel, sampleIndex })
      setFeedback({
        title: '标定草稿已更新',
        detail: `已删除 ${channelLabel(channel)} 样本。`,
        tone: 'info',
      })
    } catch (error) {
      setFeedback({ title: '标定失败', detail: errorMessage(error), tone: 'warning' })
    }
  }

  const handleCalibrationClear = async (channel: CalibrationChannel) => {
    try {
      await updateCalibrationDraft({ op: 'clear', channel })
      setFeedback({
        title: '标定草稿已更新',
        detail: `${channelLabel(channel)} 草稿已清空。`,
        tone: 'info',
      })
    } catch (error) {
      setFeedback({ title: '标定失败', detail: errorMessage(error), tone: 'warning' })
    }
  }

  const handleCalibrationManualFit = async (
    channel: CalibrationChannel,
    gain: number,
    offsetMv: number
  ) => {
    try {
      const calibrationPackage = calibrationPackageWithManualFit(
        visibleCalibration.draft,
        channel,
        gain,
        offsetMv
      )
      await updateCalibrationDraft({ op: 'import', package: calibrationPackage })
      setFeedback({
        title: '标定草稿已更新',
        detail: `${channelLabel(channel)} 草稿拟合已设为 ${gain.toFixed(5)}x / ${offsetMv.toFixed(
          1
        )}mV.`,
        tone: 'success',
      })
      emitEvent('calibration', `updated ${channelLabel(channel)} draft fit`, 'success')
    } catch (error) {
      setFeedback({ title: '标定失败', detail: errorMessage(error), tone: 'warning' })
    }
  }

  const handleCalibrationImport = async (calibrationPackage: CalibrationPackage) => {
    try {
      await updateCalibrationDraft({ op: 'import', package: calibrationPackage })
      setFeedback({
        title: '标定数据已导入',
        detail: '草稿样本已由 JSON 内容替换。',
        tone: 'success',
      })
    } catch (error) {
      setFeedback({ title: '标定失败', detail: errorMessage(error), tone: 'warning' })
    }
  }

  const handleCalibrationApply = async () => {
    if (visibleDevice.heaterEnabled || visibleDevice.heaterOutputPercent !== 0) {
      setFeedback({
        title: '应用标定被阻止',
        detail: '应用 ADC 标定前请先关闭加热。',
        tone: 'warning',
      })
      return
    }
    try {
      if (isDirectWebSerialDevice(visibleDevice)) {
        const calibration = await webSerial.applyCalibration()
        setCalibrationByDevice((current) => ({ ...current, [visibleDevice.id]: calibration }))
      } else if (visibleDevice.transport === 'devd' && visibleDevice.leaseId && devdBaseUrl) {
        const calibration = await controlClient.applyCalibration(devdBaseUrl, visibleDevice.id, {
          leaseId: visibleDevice.leaseId,
        })
        setCalibrationByDevice((current) => ({ ...current, [visibleDevice.id]: calibration }))
      } else {
        setCalibrationByDevice((current) => ({
          ...current,
          [visibleDevice.id]: {
            ...visibleCalibration,
            active: cloneCalibrationPackage(visibleCalibration.draft),
            activeFit: createCalibrationFits(visibleCalibration.draft),
          },
        }))
      }
      setFeedback({
        title: '标定已应用',
        detail: '当前 ADC 标定已与草稿一致。',
        tone: 'success',
      })
      emitEvent('calibration', 'applied ADC calibration', 'success')
    } catch (error) {
      setFeedback({ title: '应用标定失败', detail: errorMessage(error), tone: 'warning' })
    }
  }

  const updateHeaterCurveState = useCallback(
    async (request: Omit<HeaterCurveConfigRequest, 'leaseId'>) => {
      if (isDirectWebSerialDevice(visibleDevice)) {
        if (request.op === 'preview' && request.package) {
          const next = await webSerial.previewHeaterCurve(request.package)
          setHeaterCurveByDevice((current) => ({ ...current, [visibleDevice.id]: next }))
          return next
        }
        if (request.op === 'clear_preview') {
          const next = await webSerial.clearHeaterCurvePreview()
          setHeaterCurveByDevice((current) => ({ ...current, [visibleDevice.id]: next }))
          return next
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
        setHeaterCurveByDevice((current) => ({ ...current, [visibleDevice.id]: next }))
        return next
      }

      const next = applyLocalHeaterCurveRequest(visibleHeaterCurve, request)
      setHeaterCurveByDevice((current) => ({ ...current, [visibleDevice.id]: next }))
      return next
    },
    [
      controlClient,
      devdBaseUrl,
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
      setFeedback({ title: '加热曲线操作失败', detail: errorMessage(error), tone: 'warning' })
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
      setFeedback({ title: '加热曲线操作失败', detail: errorMessage(error), tone: 'warning' })
    }
  }

  const handleHeaterCurveSave = async () => {
    try {
      if (isDirectWebSerialDevice(visibleDevice)) {
        const next = await webSerial.saveHeaterCurve()
        setHeaterCurveByDevice((current) => ({ ...current, [visibleDevice.id]: next }))
      } else if (visibleDevice.transport === 'devd' && visibleDevice.leaseId && devdBaseUrl) {
        const next = await controlClient.saveHeaterCurve(devdBaseUrl, visibleDevice.id, {
          leaseId: visibleDevice.leaseId,
        })
        setHeaterCurveByDevice((current) => ({ ...current, [visibleDevice.id]: next }))
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
      setFeedback({ title: '加热曲线操作失败', detail: errorMessage(error), tone: 'warning' })
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
      request: { op: 'start' | 'cancel'; kind?: 'vin_adc_auto' | 'heater_curve_auto' },
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
    [controlClient, devdBaseUrl, emitEvent, visibleDevice, webSerial]
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
              return (
                <button
                  key={view.id}
                  type="button"
                  className={isActive ? 'industrial-view-tab is-selected' : 'industrial-view-tab'}
                  aria-pressed={isActive}
                  onClick={() => handleGuardedViewChange(view.id)}
                >
                  <Icon size={18} aria-hidden="true" />
                  <span>
                    <strong>{view.label}</strong>
                    <small>{view.caption}</small>
                  </span>
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
                onManualPpsApply={handleManualPpsApply}
                onManualPpsClear={handleManualPpsClear}
                onHeaterHoldToggle={handleHeaterHoldToggle}
                onArtifactChange={handleArtifactChange}
                onCalibrationReferenceChange={setCalibrationReference}
                onCalibrationCapture={handleCalibrationCapture}
                onCalibrationDelete={handleCalibrationDelete}
                onCalibrationClear={handleCalibrationClear}
                onCalibrationManualFit={handleCalibrationManualFit}
                onCalibrationImport={handleCalibrationImport}
                onCalibrationApply={handleCalibrationApply}
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
                onCalibrationLeaveGuardDismiss={dismissCalibrationLeaveGuard}
                onDeviceSelect={handleDeviceChange}
                onQuickAddDevice={handleQuickAddDevice}
                onAddDevice={handleAddDevice}
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

function createDefaultCalibrationState(): CalibrationState {
  const empty = createEmptyCalibrationPackage()
  return {
    active: cloneCalibrationPackage(empty),
    draft: cloneCalibrationPackage(empty),
    activeFit: createCalibrationFits(empty),
    draftFit: createCalibrationFits(empty),
  }
}

function createEmptyCalibrationPackage(): CalibrationPackage {
  return {
    rtdAdc: Array.from({ length: 8 }, () => null),
    vinAdc: Array.from({ length: 8 }, () => null),
  }
}

function isRtdCalibrationSample(
  sample: RtdCalibrationSample | VinCalibrationSample
): sample is RtdCalibrationSample {
  return 'referenceTempC' in sample
}

function formatRtdCalibrationReference(sample: RtdCalibrationSample) {
  if (sample.referenceTempC != null) {
    return `${sample.referenceTempC.toFixed(1)}℃`
  }
  return `${rtdTemperatureForAdcMv(sample.expectedMv).toFixed(1)}℃`
}

function formatRtdCalibrationTargetAdc(sample: RtdCalibrationSample) {
  const targetAdcMv = sample.targetAdcMv ?? sample.expectedMv
  return `${targetAdcMv}mV`
}

function cloneCalibrationPackage(calibrationPackage: CalibrationPackage): CalibrationPackage {
  return {
    rtdAdc: calibrationPackage.rtdAdc.map((sample) => (sample ? { ...sample } : null)),
    vinAdc: calibrationPackage.vinAdc.map((sample) => (sample ? { ...sample } : null)),
  }
}

function applyLocalCalibrationRuntimeRequest(
  current: CalibrationRuntimeState,
  request: CalibrationControlRequest
): CalibrationRuntimeState {
  const nextMode = request.mode ?? current.mode
  const nextPpsEnabled = request.ppsEnabled ?? current.ppsEnabled
  const nextHeaterEnabled =
    nextMode === 'off' ? false : (request.heaterEnabled ?? current.heaterEnabled)
  const nextTargetAdcMv = request.targetAdcMv ?? current.targetAdcMv ?? null

  return {
    ...current,
    mode: nextMode,
    ppsEnabled: nextPpsEnabled,
    ppsMv: request.ppsEnabled === false ? null : (request.ppsMv ?? current.ppsMv ?? null),
    ppsMa: request.ppsEnabled === false ? null : current.ppsMa,
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

function createCalibrationFits(
  calibrationPackage: CalibrationPackage
): CalibrationState['activeFit'] {
  return {
    rtdAdc: createCalibrationFit(calibrationPackage.rtdAdc, 'rtd_adc'),
    vinAdc: createCalibrationFit(calibrationPackage.vinAdc, 'vin_adc'),
  }
}

function createCalibrationFit(
  samples: Array<BaseCalibrationSample | null>,
  channel: CalibrationChannel
) {
  const custom = samples.filter((sample): sample is BaseCalibrationSample => Boolean(sample))
  const defaults =
    channel === 'rtd_adc'
      ? [
          { observedMv: 0, expectedMv: 0 },
          { observedMv: 2800, expectedMv: 2800 },
        ]
      : [
          { observedMv: 0, expectedMv: 0 },
          { observedMv: 2337, expectedMv: 2337 },
        ]
  const points = custom.length < 2 ? [...defaults, ...custom] : custom
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
    customSampleCount: custom.length,
    defaultSampleCount: custom.length < 2 ? 2 : 0,
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

function applyLocalCalibrationRequest(
  current: CalibrationState,
  request: Omit<CalibrationConfigRequest, 'leaseId'>
): CalibrationState {
  const draft = cloneCalibrationPackage(current.draft)
  if (request.op === 'import') {
    const imported = request.package ? normalizeCalibrationPackage(request.package) : draft
    return {
      ...current,
      draft: imported,
      draftFit: createCalibrationFits(imported),
    }
  }
  const channel = request.channel
  if (!channel) {
    throw new Error('缺少标定通道。')
  }
  const samples = channel === 'rtd_adc' ? draft.rtdAdc : draft.vinAdc
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
      (channel === 'rtd_adc'
        ? rtdAdcMvForTemperature(request.referenceTempC ?? 0)
        : vinAdcMvForInput(request.referenceVinMv ?? 0))
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
  }
  const normalized = normalizeCalibrationPackage(draft)
  return {
    ...current,
    draft: normalized,
    draftFit: createCalibrationFits(normalized),
  }
}

function calibrationPackageWithManualFit(
  currentDraft: CalibrationPackage,
  channel: CalibrationChannel,
  gain: number,
  offsetMv: number
): CalibrationPackage {
  const samples = manualFitSamples(gain, offsetMv)
  const next = cloneCalibrationPackage(currentDraft)
  if (channel === 'rtd_adc') {
    next.rtdAdc = samples
  } else {
    next.vinAdc = samples
  }
  return next
}

function manualFitSamples(gain: number, offsetMv: number): CalibrationPackage['rtdAdc'] {
  if (!Number.isFinite(gain) || gain <= 0 || !Number.isFinite(offsetMv)) {
    throw new Error('手动拟合要求增益大于 0，且偏移量必须是有限数值。')
  }

  const low = Math.max(0, Math.ceil(offsetMv < 0 ? (-offsetMv + 1) / gain : 0))
  const high = Math.min(65_535, Math.floor((65_535 - offsetMv) / gain))
  if (high <= low) {
    throw new Error('手动拟合结果超出了 ADC 毫伏范围。')
  }

  const points = Array.from({ length: 8 }, (_, index) => {
    const observedMv = Math.round(low + ((high - low) * index) / 7)
    return {
      observedMv,
      expectedMv: Math.round(gain * observedMv + offsetMv),
    }
  })

  if (
    high > 65_535 ||
    points.some(
      (sample) =>
        sample.observedMv < 0 ||
        sample.observedMv > 65_535 ||
        sample.expectedMv < 0 ||
        sample.expectedMv > 65_535
    )
  ) {
    throw new Error('手动拟合结果超出了 ADC 毫伏范围。')
  }

  return points
}

function normalizeCalibrationPackage(calibrationPackage: CalibrationPackage): CalibrationPackage {
  const normalize = <TSample extends BaseCalibrationSample>(
    samples: Array<TSample | null>
  ): Array<TSample | null> => {
    const compacted = samples.filter(Boolean) as TSample[]
    return Array.from({ length: 8 }, (_, index) => compacted[index] ?? null)
  }
  return {
    rtdAdc: normalize(calibrationPackage.rtdAdc),
    vinAdc: normalize(calibrationPackage.vinAdc),
  }
}

function vinAdcMvForInput(inputMv: number) {
  return Math.round((inputMv * 5100) / (56_000 + 5100))
}

function channelLabel(channel: CalibrationChannel) {
  return channel === 'rtd_adc' ? '温度 ADC' : '电压 ADC'
}

function calibrationFitMode(fit: CalibrationState['activeFit']['rtdAdc']) {
  if (fit.customSampleCount >= 2) {
    return '自定义'
  }
  if (fit.customSampleCount === 1) {
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

function isTransportBlockedFeedback(current: ActionFeedback) {
  return (
    current.title === '设备租约冲突' ||
    current.title === '硬件连接受阻' ||
    current.title === '目标温度更新被阻止'
  )
}

function isNoLiveTargetDevice(device: DeviceTarget) {
  return device.id === NO_LIVE_TARGET_ID && device.transport === 'serial'
}

function isKnownDeviceChoice(device: DeviceTarget) {
  return !isNoLiveTargetDevice(device) && !isDirectWebSerialDevice(device)
}

function isPendingDeviceChoice(device: DeviceTarget) {
  return device.id.startsWith('pending-')
}

function isLiveRuntimeDevice(device: Pick<DeviceTarget, 'transport' | 'baseUrl'>) {
  return device.transport === 'devd' || isDirectWebSerialDevice(device)
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
  if (value < 0) {
    return 'N/A'
  }

  return `${formatTempNumber(value)}℃`
}

function formatPresetTemp(value: number, enabled: boolean) {
  return enabled ? `${formatTempNumber(value)}℃` : '---'
}

function formatTempNumber(value: number) {
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
        <Select value={device.id} onValueChange={onDeviceChange}>
          <SelectTrigger
            aria-label="目标设备"
            className="industrial-device-select industrial-radix-select"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent className="industrial-select-content">
            {devices.map((item) => (
              <SelectItem key={item.id} value={item.id} className="industrial-select-item">
                {item.alias} / {transportLabels[item.transport]}
              </SelectItem>
            ))}
            <SelectSeparator className="industrial-select-separator" />
            <SelectItem
              value={ADD_DEVICE_VALUE}
              className="industrial-select-item industrial-select-item--add"
            >
              添加设备
            </SelectItem>
          </SelectContent>
        </Select>
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

function ViewPanel({
  view,
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
  onManualPpsApply,
  onManualPpsClear,
  onHeaterHoldToggle,
  onArtifactChange,
  onDeviceSelect,
  onQuickAddDevice,
  onAddDevice,
  onStartDryRun,
  onStartFlash,
  onCalibrationReferenceChange,
  onCalibrationCapture,
  onCalibrationDelete,
  onCalibrationClear,
  onCalibrationManualFit,
  onCalibrationImport,
  onCalibrationApply,
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
}: {
  view: ConsoleView
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
  onManualPpsApply: (millivolts: number) => void | Promise<void>
  onManualPpsClear: () => void | Promise<void>
  onHeaterHoldToggle: () => void
  onArtifactChange: (artifactId: string) => void
  onDeviceSelect: (deviceId: string) => void
  onQuickAddDevice: (kind: AddDeviceKind) => void
  onAddDevice: (kind: AddDeviceKind) => void
  onStartDryRun: () => void
  onStartFlash: () => void
  onCalibrationReferenceChange: (channel: CalibrationChannel, value: number) => void
  onCalibrationCapture: (
    channel: CalibrationChannel,
    options?: { targetAdcMv?: number }
  ) => void | Promise<void>
  onCalibrationDelete: (channel: CalibrationChannel, sampleIndex: number) => void | Promise<void>
  onCalibrationClear: (channel: CalibrationChannel) => void | Promise<void>
  onCalibrationManualFit: (
    channel: CalibrationChannel,
    gain: number,
    offsetMv: number
  ) => void | Promise<void>
  onCalibrationImport: (calibrationPackage: CalibrationPackage) => void | Promise<void>
  onCalibrationApply: () => void | Promise<void>
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
    request: { op: 'start' | 'cancel'; kind?: 'vin_adc_auto' | 'heater_curve_auto' },
    failureMessage: string
  ) => void | Promise<void>
  onHeaterCurvePreview: (heaterCurve: HeaterCurvePackage) => void | Promise<void>
  onHeaterCurveClearPreview: () => void | Promise<void>
  onHeaterCurveSave: () => void | Promise<void>
  onCalibrationWorkspaceTabChange: (nextTab: CalibrationWorkspaceTab) => void
  calibrationLeaveGuard: CalibrationLeaveGuardState | null
  onCalibrationLeaveGuardDismiss: () => void
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
        onClear={onCalibrationClear}
        onManualFit={onCalibrationManualFit}
        onImport={onCalibrationImport}
        onApply={onCalibrationApply}
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
  return (
    <div className="industrial-view-panel industrial-device-select-view">
      <PanelHeader kicker="Device" title="Choose target" />
      <section className="industrial-device-select-section" aria-label="Known devices">
        {knownDevices.length > 0 ? (
          <div className="industrial-known-device-grid">
            {knownDevices.map((device) => (
              <button
                key={device.id}
                type="button"
                className="industrial-known-device-card"
                onClick={() => onDeviceSelect(device.id)}
              >
                <span>
                  <strong>{device.alias}</strong>
                  <small>{device.location}</small>
                </span>
                <em>{transportLabels[device.transport]}</em>
                <b>{severityLabels[device.severity]}</b>
              </button>
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
}: {
  allowDemoControls: boolean
  webSerial: Pick<LiveWebSerialControls, 'state' | 'supported'>
  feedback: ActionFeedback
  onAddDevice: (kind: AddDeviceKind) => void
}) {
  return (
    <div className="industrial-view-panel industrial-view-panel--calibration">
      <PanelHeader kicker="Add device" title="Choose connection" />
      <AddDeviceChoices
        allowDemoControls={allowDemoControls}
        webSerial={webSerial}
        onAddDevice={onAddDevice}
      />
      <ActionFeedbackPanel feedback={feedback} />
    </div>
  )
}

function AddDeviceChoices({
  allowDemoControls,
  webSerial,
  onAddDevice,
}: {
  allowDemoControls: boolean
  webSerial: Pick<LiveWebSerialControls, 'state' | 'supported'>
  onAddDevice: (kind: AddDeviceKind) => void
}) {
  const webSerialDisabled =
    !allowDemoControls &&
    (webSerial.state === 'unsupported' ||
      webSerial.state === 'connecting' ||
      webSerial.state === 'connected')

  return (
    <div className="industrial-add-device-grid">
      {addDeviceOptions.map((item) => {
        const disabled = item.kind === 'web-serial' && webSerialDisabled
        const label =
          item.kind === 'web-serial' && webSerial.state === 'connecting'
            ? 'Web Serial (connecting)'
            : item.kind === 'web-serial' && webSerial.state === 'connected'
              ? 'Web Serial connected'
              : item.kind === 'web-serial' && !allowDemoControls && !webSerial.supported
                ? 'Web Serial unavailable'
                : item.label

        return (
          <button
            key={item.kind}
            type="button"
            className="industrial-add-device-option"
            disabled={disabled}
            onClick={() => onAddDevice(item.kind)}
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
}: {
  device: DeviceTarget
  artifact?: FirmwareArtifact
  feedback: ActionFeedback
  onTargetTempChange: (nextTargetTemp: number) => void
  onManualPpsApply: (millivolts: number) => void | Promise<void>
  onManualPpsClear: () => void | Promise<void>
  onHeaterHoldToggle: () => void
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
  return (
    <div className="industrial-view-panel">
      <PanelHeader kicker="Dashboard" title="Thermal runtime" />
      <div className="industrial-runtime-surface">
        <section className={`industrial-temp-dial is-${temperatureBand(device.currentTempC)}`}>
          <p className="industrial-label">Current temp</p>
          <div className="industrial-temp-dial__value">
            <strong>{formatTempNumber(device.currentTempC)}</strong>
            <span>℃</span>
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
          disabled={device.severity === 'offline'}
          onChange={onTargetTempChange}
        />
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
          disabled={device.severity === 'offline'}
          onClick={onHeaterHoldToggle}
        >
          <Power size={16} aria-hidden="true" />
          {device.heaterEnabled ? 'Hold heater' : 'Resume heater'}
        </button>
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
  const disabled = device.severity === 'offline' || !range || maxMa == null
  const clearDisabled = device.severity === 'offline' || !device.manualPpsEnabled
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
}) {
  const numericValue = Number(valueText)
  const sliderValue = Number.isFinite(numericValue)
    ? Math.min(Math.max(numericValue, min), max)
    : min

  return (
    <div className="industrial-calibration-field industrial-calibration-slider-field">
      <div className="industrial-calibration-slider-field__header">
        <span>{label}</span>
        <span className="industrial-calibration-input industrial-calibration-input--compact">
          <input
            type="number"
            inputMode="numeric"
            step={step}
            min={min}
            max={max}
            value={valueText}
            disabled={disabled}
            aria-label={inputAriaLabel}
            onChange={(event) => onChange(event.currentTarget.value)}
          />
          <small>{unit}</small>
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
      modeToggleHint={
        leaveGuard ? (
          <CalibrationLeaveGuardBubble
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
}) {
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
  onClear,
  onManualFit,
  onImport,
  onApply,
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
}: {
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
    options?: { targetAdcMv?: number }
  ) => void | Promise<void>
  onDelete: (channel: CalibrationChannel, sampleIndex: number) => void | Promise<void>
  onClear: (channel: CalibrationChannel) => void | Promise<void>
  onManualFit: (channel: CalibrationChannel, gain: number, offsetMv: number) => void | Promise<void>
  onImport: (calibrationPackage: CalibrationPackage) => void | Promise<void>
  onApply: () => void | Promise<void>
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
    request: { op: 'start' | 'cancel'; kind?: 'vin_adc_auto' | 'heater_curve_auto' },
    failureMessage: string
  ) => void | Promise<void>
  onHeaterCurvePreview: (heaterCurve: HeaterCurvePackage) => void | Promise<void>
  onHeaterCurveClearPreview: () => void | Promise<void>
  onHeaterCurveSave: () => void | Promise<void>
  onCalibrationWorkspaceTabChange: (nextTab: CalibrationWorkspaceTab) => void
  calibrationLeaveGuard: CalibrationLeaveGuardState | null
  onCalibrationLeaveGuardDismiss: () => void
}) {
  const fileInputRef = useRef<HTMLInputElement | null>(null)
  const [heaterCurveDraftText, setHeaterCurveDraftText] = useState('')
  const [vinPpsMvText, setVinPpsMvText] = useState('')
  const [rtdPpsMvText, setRtdPpsMvText] = useState('')
  const [rtdTargetAdcText, setRtdTargetAdcText] = useState('')
  const [heaterPpsMvText, setHeaterPpsMvText] = useState('')
  const [pendingCalibrationAction, setPendingCalibrationAction] = useState<string | null>(null)
  const lastRtdDraftDeviceIdRef = useRef<string | null>(null)
  const lastLiveRtdTargetAdcMvRef = useRef<number | null>(null)
  const rtdTargetAdcCommitTimerRef = useRef<number | null>(null)
  const rtdTargetAdcCommitVersionRef = useRef(0)
  const transportBlockedReason = deviceControlBlockReason(device)
  const controlsBlocked = transportBlockedReason != null
  const applyBlocked = controlsBlocked || device.heaterEnabled || device.heaterOutputPercent !== 0
  const requestedWorkbenchMode = calibrationWorkspaceTab
  const activeWorkbenchMode = asWorkbenchMode(runtimeCalibration.mode)
  const modeArmed = activeWorkbenchMode === requestedWorkbenchMode
  const ppsRange = ppsCapabilityRange(device)
  const hasPpsCapability = ppsRange != null
  const powerCapability = calibrationPowerCapability(device)
  const basePpsDraft = calibrationPpsDraft(device, runtimeCalibration)

  const exportCalibration = () => {
    const blob = new Blob([JSON.stringify(calibration, null, 2)], { type: 'application/json' })
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
    const parsed = JSON.parse(await file.text()) as
      | CalibrationState
      | { package: CalibrationPackage }
    const calibrationPackage =
      'draft' in parsed ? parsed.draft : 'package' in parsed ? parsed.package : null
    if (calibrationPackage) {
      await onImport(calibrationPackage)
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

  const parseIntegerInput = (rawValue: string) => {
    if (rawValue.trim() === '') {
      return null
    }
    const next = Number(rawValue)
    return Number.isFinite(next) ? Math.round(next) : null
  }

  const currentModeError = runtimeCalibration.error ?? null
  const currentJob = runtimeCalibration.job
  const jobRunning = currentJob.status === 'running'
  const actionLockTimerRef = useRef<number | null>(null)

  const vinPpsMv = parseIntegerInput(vinPpsMvText)
  const vinPpsError =
    vinPpsMv == null ? '请输入整数 PPS 电压。' : validateCalibrationPpsInput(device, vinPpsMv)
  const vinCanSubmitPps = hasPpsCapability && vinPpsError == null

  const rtdPpsMv = parseIntegerInput(rtdPpsMvText)
  const rtdTargetAdcMv = parseIntegerInput(rtdTargetAdcText)
  const rtdPpsError =
    rtdPpsMv == null ? '请输入整数 PPS 电压。' : validateCalibrationPpsInput(device, rtdPpsMv)
  const rtdTargetError =
    rtdTargetAdcMv == null || rtdTargetAdcMv < 0 ? '目标 ADC 必须是非负毫伏值。' : null
  const rtdCanSubmitRuntime = hasPpsCapability && rtdPpsError == null && rtdTargetError == null

  const heaterPpsMv = parseIntegerInput(heaterPpsMvText)
  const heaterPpsError =
    heaterPpsMv == null ? '请输入整数 PPS 电压。' : validateCalibrationPpsInput(device, heaterPpsMv)
  const heaterCanSubmitPps = hasPpsCapability && heaterPpsError == null

  useEffect(
    () => () => {
      if (rtdTargetAdcCommitTimerRef.current != null) {
        window.clearTimeout(rtdTargetAdcCommitTimerRef.current)
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
      rtdTargetAdcMv == null ||
      rtdTargetError != null ||
      runtimeCalibration.targetAdcMv === rtdTargetAdcMv
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
          targetAdcMv: rtdTargetAdcMv,
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
    modeArmed,
    onCalibrationRuntimeChange,
    pendingCalibrationAction,
    rtdTargetAdcMv,
    rtdTargetError,
    runtimeCalibration.mode,
    runtimeCalibration.ppsEnabled,
    runtimeCalibration.targetAdcMv,
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
  const leaveGuardViewModel = calibrationLeaveGuard
    ? {
        nextLabel: calibrationLeaveGuard.nextLabel,
        onDismiss: onCalibrationLeaveGuardDismiss,
        onContinue: async () => {
          const continueAction = calibrationLeaveGuard.continueAction
          const exited = await onModeExit()
          if (!exited) {
            return
          }
          onCalibrationLeaveGuardDismiss()
          await continueAction()
        },
      }
    : null

  const adcApplyToolbar = (
    <AdcCalibrationToolbar
      applyBlocked={applyBlocked}
      disabled={controlsBlocked}
      feedback={feedback}
      onApply={onApply}
      onExport={exportCalibration}
      onImport={() => fileInputRef.current?.click()}
    />
  )

  return (
    <div className="industrial-view-panel industrial-view-panel--calibration-workbench">
      <div className="industrial-calibration-workbench">
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
            <TabsTrigger value="heater_curve" className="industrial-calibration-tab">
              加热曲线标定
            </TabsTrigger>
            <TabsTrigger value="rtd_adc" className="industrial-calibration-tab">
              温度标定
            </TabsTrigger>
            <TabsTrigger value="vin_adc" className="industrial-calibration-tab">
              电压读数标定
            </TabsTrigger>
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
                        disabled={controlsBlocked}
                        onEnable={() =>
                          void onModeEnter('heater_curve', {
                            mode: 'heater_curve',
                            ppsEnabled: false,
                            heaterEnabled: false,
                          })
                        }
                        onDisable={() => onModeExit()}
                      />
                    }
                    leaveGuard={leaveGuardViewModel}
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
                        id: 'heater-pps-toggle',
                        node: (
                          <button
                            type="button"
                            className="industrial-button industrial-button--secondary"
                            disabled={
                              controlsBlocked ||
                              pendingCalibrationAction != null ||
                              (!runtimeCalibration.ppsEnabled && !heaterCanSubmitPps)
                            }
                            onClick={() =>
                              void runCalibrationAction('heater-pps-toggle', () =>
                                onCalibrationRuntimeChange(
                                  runtimeCalibration.ppsEnabled
                                    ? {
                                        mode: 'heater_curve',
                                        ppsEnabled: false,
                                      }
                                    : {
                                        mode: 'heater_curve',
                                        ppsEnabled: true,
                                        ppsMv: heaterPpsMv ?? undefined,
                                      },
                                  runtimeCalibration.ppsEnabled
                                    ? 'PPS 停止失败。'
                                    : 'PPS 请求超出能力范围。'
                                )
                              )
                            }
                          >
                            {calibrationActionPending('heater-pps-toggle')
                              ? '处理中...'
                              : runtimeCalibration.ppsEnabled
                                ? '关闭 PPS'
                                : '申请 PPS'}
                          </button>
                        ),
                      },
                      {
                        id: 'heater-job-toggle',
                        node: (
                          <button
                            type="button"
                            className="industrial-button industrial-button--secondary"
                            disabled={
                              controlsBlocked ||
                              pendingCalibrationAction != null ||
                              (!jobRunning && (!modeArmed || !heaterCanSubmitPps))
                            }
                            onClick={() =>
                              void runCalibrationAction('heater-job-toggle', () =>
                                onCalibrationJobChange(
                                  jobRunning
                                    ? { op: 'cancel' }
                                    : { op: 'start', kind: 'heater_curve_auto' },
                                  jobRunning
                                    ? '加热曲线自动采样取消失败。'
                                    : '加热曲线自动采样启动失败。'
                                )
                              )
                            }
                          >
                            {calibrationActionPending('heater-job-toggle')
                              ? '处理中...'
                              : jobRunning
                                ? '取消校准'
                                : '自动校准'}
                          </button>
                        ),
                      },
                      {
                        id: 'heater-heater-toggle',
                        node: (
                          <button
                            type="button"
                            className="industrial-button industrial-button--secondary"
                            disabled={
                              controlsBlocked || !modeArmed || pendingCalibrationAction != null
                            }
                            onClick={() =>
                              void runCalibrationAction('heater-heater-toggle', () =>
                                onCalibrationRuntimeChange(
                                  {
                                    mode: 'heater_curve',
                                    heaterEnabled: !runtimeCalibration.heaterEnabled,
                                  },
                                  '加热切换失败。'
                                )
                              )
                            }
                          >
                            {calibrationActionPending('heater-heater-toggle')
                              ? '处理中...'
                              : runtimeCalibration.heaterEnabled
                                ? '关闭加热'
                                : '开启加热'}
                          </button>
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
                      disabled={controlsBlocked || !modeArmed || pendingCalibrationAction != null}
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
                </div>
              </div>
              {adcApplyToolbar}
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
                            ppsEnabled: false,
                            heaterEnabled: false,
                            targetAdcMv:
                              rtdTargetAdcMv ??
                              runtimeCalibration.targetAdcMv ??
                              device.rtdRawAdcMv ??
                              undefined,
                          })
                        }
                        onDisable={() => onModeExit()}
                      />
                    }
                    leaveGuard={leaveGuardViewModel}
                    capability={powerCapability}
                    voltageText={rtdPpsMvText}
                    onVoltageChange={setRtdPpsMvText}
                    range={ppsRange}
                    hasPpsCapability={hasPpsCapability}
                    errors={
                      <>
                        {rtdPpsError ? (
                          <p className="industrial-calibration-inline-error">{rtdPpsError}</p>
                        ) : null}
                        {rtdTargetError ? (
                          <p className="industrial-calibration-inline-error">{rtdTargetError}</p>
                        ) : null}
                      </>
                    }
                    actionSlots={[
                      {
                        id: 'rtd-pps-toggle',
                        node: (
                          <button
                            type="button"
                            className="industrial-button industrial-button--secondary"
                            disabled={
                              controlsBlocked ||
                              pendingCalibrationAction != null ||
                              (!runtimeCalibration.ppsEnabled && !rtdCanSubmitRuntime)
                            }
                            onClick={() =>
                              void runCalibrationAction('rtd-hold-toggle', () =>
                                onCalibrationRuntimeChange(
                                  runtimeCalibration.ppsEnabled
                                    ? {
                                        mode: 'rtd_adc',
                                        ppsEnabled: false,
                                      }
                                    : {
                                        mode: 'rtd_adc',
                                        ppsEnabled: true,
                                        ppsMv: rtdPpsMv ?? undefined,
                                        targetAdcMv: rtdTargetAdcMv ?? undefined,
                                      },
                                  runtimeCalibration.ppsEnabled
                                    ? '温度保持停止失败。'
                                    : '温度保持请求非法。'
                                )
                              )
                            }
                          >
                            {calibrationActionPending('rtd-hold-toggle')
                              ? '处理中...'
                              : runtimeCalibration.ppsEnabled
                                ? '关闭 PPS'
                                : '申请 PPS'}
                          </button>
                        ),
                      },
                      {
                        id: 'rtd-job-placeholder',
                        node: (
                          <span
                            className="industrial-calibration-inline-actions__placeholder"
                            aria-hidden="true"
                          />
                        ),
                      },
                      {
                        id: 'rtd-heater-toggle',
                        node: (
                          <button
                            type="button"
                            className="industrial-button industrial-button--secondary"
                            disabled={
                              controlsBlocked || !modeArmed || pendingCalibrationAction != null
                            }
                            onClick={() =>
                              void runCalibrationAction('rtd-heater-toggle', () =>
                                onCalibrationRuntimeChange(
                                  {
                                    mode: 'rtd_adc',
                                    heaterEnabled: !runtimeCalibration.heaterEnabled,
                                  },
                                  '加热切换失败。'
                                )
                              )
                            }
                          >
                            {calibrationActionPending('rtd-heater-toggle')
                              ? '处理中...'
                              : runtimeCalibration.heaterEnabled
                                ? '关闭加热'
                                : '开启加热'}
                          </button>
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
                    />
                  </CalibrationModeControlPanel>
                </div>
                <div className="industrial-calibration-side-stack">
                  <CalibrationWorkbenchCard
                    title="状态"
                    summary={
                      <CalibrationFitStatusSummary
                        liveLabel="当前 ADC"
                        liveValue={
                          device.rtdRawAdcMv != null ? `${device.rtdRawAdcMv}mV` : '未采样'
                        }
                        activeFit={calibration.activeFit.rtdAdc}
                        draftFit={calibration.draftFit.rtdAdc}
                      />
                    }
                    guidance={
                      rtdTargetError
                        ? '先修正目标 ADC，再继续采集或调整草稿拟合。'
                        : '先确认目标 ADC 稳定，再写入温度样本。'
                    }
                    messages={
                      <>
                        {rtdTargetError ? (
                          <p className="industrial-calibration-inline-error">{rtdTargetError}</p>
                        ) : null}
                        {currentModeError ? (
                          <p className="industrial-calibration-inline-error">{currentModeError}</p>
                        ) : null}
                      </>
                    }
                  >
                    <CalibrationChannelControls
                      title="温度 ADC"
                      referenceLabel="参考温度"
                      referenceValue={refs.rtdTempC}
                      referenceUnit="℃"
                      draftFit={calibration.draftFit.rtdAdc}
                      samples={calibration.draft.rtdAdc}
                      disabled={controlsBlocked}
                      onReferenceChange={(value) => onReferenceChange('rtd_adc', value)}
                      onCapture={() =>
                        onCapture('rtd_adc', {
                          targetAdcMv:
                            rtdTargetAdcMv ??
                            runtimeCalibration.targetAdcMv ??
                            device.rtdRawAdcMv ??
                            undefined,
                        })
                      }
                      onClear={() => onClear('rtd_adc')}
                      onManualFit={(gain, offsetMv) => onManualFit('rtd_adc', gain, offsetMv)}
                    />
                  </CalibrationWorkbenchCard>
                </div>
              </div>
              {adcApplyToolbar}
              <section className="industrial-calibration-channel industrial-calibration-channel--samples">
                <CalibrationChannelSamples
                  channel="rtd_adc"
                  title="温度 ADC"
                  samples={calibration.draft.rtdAdc}
                  disabled={controlsBlocked}
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
                            ppsEnabled: false,
                            heaterEnabled: false,
                          })
                        }
                        onDisable={() => onModeExit()}
                      />
                    }
                    leaveGuard={leaveGuardViewModel}
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
                        id: 'vin-pps-toggle',
                        node: (
                          <button
                            type="button"
                            className="industrial-button industrial-button--secondary"
                            disabled={
                              controlsBlocked ||
                              pendingCalibrationAction != null ||
                              (!runtimeCalibration.ppsEnabled && !vinCanSubmitPps)
                            }
                            onClick={() =>
                              void runCalibrationAction('vin-pps-toggle', () =>
                                onCalibrationRuntimeChange(
                                  runtimeCalibration.ppsEnabled
                                    ? {
                                        mode: 'vin_adc',
                                        ppsEnabled: false,
                                      }
                                    : {
                                        mode: 'vin_adc',
                                        ppsEnabled: true,
                                        ppsMv: vinPpsMv ?? undefined,
                                      },
                                  runtimeCalibration.ppsEnabled
                                    ? 'VIN 标定 PPS 停止失败。'
                                    : 'VIN 标定 PPS 请求非法。'
                                )
                              )
                            }
                          >
                            {calibrationActionPending('vin-pps-toggle')
                              ? '处理中...'
                              : runtimeCalibration.ppsEnabled
                                ? '关闭 PPS'
                                : '申请 PPS'}
                          </button>
                        ),
                      },
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
                                  jobRunning
                                    ? '电压读数自动扫点取消失败。'
                                    : '电压读数自动扫点启动失败。'
                                )
                              )
                            }
                          >
                            {calibrationActionPending('vin-job-toggle')
                              ? '处理中...'
                              : jobRunning
                                ? '取消校准'
                                : '自动校准'}
                          </button>
                        ),
                      },
                      {
                        id: 'vin-heater-toggle',
                        node: (
                          <button
                            type="button"
                            className="industrial-button industrial-button--secondary"
                            disabled={
                              controlsBlocked || !modeArmed || pendingCalibrationAction != null
                            }
                            onClick={() =>
                              void runCalibrationAction('vin-heater-toggle', () =>
                                onCalibrationRuntimeChange(
                                  {
                                    mode: 'vin_adc',
                                    heaterEnabled: !runtimeCalibration.heaterEnabled,
                                  },
                                  '加热切换失败。'
                                )
                              )
                            }
                          >
                            {calibrationActionPending('vin-heater-toggle')
                              ? '处理中...'
                              : runtimeCalibration.heaterEnabled
                                ? '关闭加热'
                                : '开启加热'}
                          </button>
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
                      disabled={controlsBlocked || !modeArmed || pendingCalibrationAction != null}
                      inputAriaLabel="电压读数标定目标温度输入"
                      sliderAriaLabel="电压读数标定目标温度滑块"
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
                  <CalibrationWorkbenchCard
                    title="状态"
                    summary={
                      <CalibrationFitStatusSummary
                        liveLabel="当前 ADC"
                        liveValue={
                          device.vinRawAdcMv != null ? `${device.vinRawAdcMv}mV` : '未采样'
                        }
                        activeFit={calibration.activeFit.vinAdc}
                        draftFit={calibration.draftFit.vinAdc}
                      />
                    }
                    guidance={
                      vinPpsError
                        ? '先修正 PPS 电压，再开始自动扫点或采样。'
                        : '右侧草稿样本会直接影响拟合结果。'
                    }
                    messages={
                      <>
                        {vinPpsError ? (
                          <p className="industrial-calibration-inline-error">{vinPpsError}</p>
                        ) : null}
                        {currentModeError ? (
                          <p className="industrial-calibration-inline-error">{currentModeError}</p>
                        ) : null}
                        {currentJob.message ? (
                          <p className="industrial-calibration-inline-error">
                            {currentJob.message}
                          </p>
                        ) : null}
                      </>
                    }
                  >
                    <CalibrationChannelControls
                      title="电压 ADC"
                      referenceLabel="参考电压"
                      referenceValue={refs.vinMv}
                      referenceUnit="mV"
                      draftFit={calibration.draftFit.vinAdc}
                      samples={calibration.draft.vinAdc}
                      disabled={controlsBlocked}
                      onReferenceChange={(value) => onReferenceChange('vin_adc', value)}
                      onCapture={() => onCapture('vin_adc')}
                      onClear={() => onClear('vin_adc')}
                      onManualFit={(gain, offsetMv) => onManualFit('vin_adc', gain, offsetMv)}
                    />
                  </CalibrationWorkbenchCard>
                </div>
              </div>
              {adcApplyToolbar}
              <section className="industrial-calibration-channel industrial-calibration-channel--samples">
                <CalibrationChannelSamples
                  channel="vin_adc"
                  title="电压 ADC"
                  samples={calibration.draft.vinAdc}
                  disabled={controlsBlocked}
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
      checked={active}
      disabled={disabled}
      onCheckedChange={(checked) => void (checked ? onEnable() : onDisable())}
    />
  )
}

function CalibrationLiveCard({
  title,
  detail,
  modeToggle,
  modeToggleHint,
  titleMeta,
  compact = false,
  children,
}: {
  title: string
  detail?: string
  modeToggle?: ReactNode
  modeToggleHint?: ReactNode
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
              <h4>{title}</h4>
              {titleMeta ?? null}
            </div>
            {detail ? <p>{detail}</p> : null}
          </div>
          <div className="industrial-calibration-live-card__mode-control">
            {modeToggleHint ?? null}
            {modeToggle ?? null}
          </div>
        </div>
      </div>
      {children}
    </section>
  )
}

function CalibrationLeaveGuardBubble({
  nextLabel,
  onDismiss,
  onContinue,
}: {
  nextLabel: string
  onDismiss: () => void
  onContinue: () => void
}) {
  const anchorRef = useRef<HTMLSpanElement | null>(null)
  const bubbleRef = useRef<HTMLDivElement | null>(null)
  const [bubbleStyle, setBubbleStyle] = useState<CSSProperties>({ visibility: 'hidden' })
  const [bubbleSide, setBubbleSide] = useState<'left' | 'bottom' | 'top'>('left')

  useLayoutEffect(() => {
    const anchor = anchorRef.current
    if (!anchor) {
      return
    }

    let frameId = 0
    const gap = 12
    const viewportMargin = 16

    const updatePosition = () => {
      const bubble = bubbleRef.current
      if (!bubble) {
        return
      }

      const anchorRect = anchor.getBoundingClientRect()
      const bubbleRect = bubble.getBoundingClientRect()
      let nextSide: 'left' | 'bottom' | 'top' = 'left'
      let left = anchorRect.left - bubbleRect.width - gap
      let top = anchorRect.top + anchorRect.height / 2 - bubbleRect.height / 2

      if (left < viewportMargin) {
        nextSide = 'bottom'
        left = anchorRect.right - bubbleRect.width
        top = anchorRect.bottom + gap

        if (top + bubbleRect.height > window.innerHeight - viewportMargin) {
          nextSide = 'top'
          top = anchorRect.top - bubbleRect.height - gap
        }
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
  }, [])

  const bubble =
    typeof document === 'undefined'
      ? null
      : createPortal(
          <div
            ref={bubbleRef}
            className="industrial-calibration-leave-guard"
            data-side={bubbleSide}
            role="alert"
            aria-live="polite"
            style={bubbleStyle}
          >
            <div className="industrial-calibration-leave-guard__header">
              <div className="industrial-calibration-leave-guard__badge">
                <AlertTriangle size={12} strokeWidth={2.3} aria-hidden="true" />
                <span>校准未关闭</span>
              </div>
              <div className="industrial-calibration-leave-guard__eyebrow">切换前提醒</div>
            </div>
            <p>校准控制仍开着，先关闭后再切到“{nextLabel}”。</p>
            <div className="industrial-calibration-leave-guard__actions">
              <button
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
  applyBlocked,
  disabled = false,
  feedback,
  onApply,
  onExport,
  onImport,
}: {
  applyBlocked: boolean
  disabled?: boolean
  feedback: ActionFeedback
  onApply: () => void | Promise<void>
  onExport: () => void
  onImport: () => void
}) {
  return (
    <section className="industrial-calibration-adc-toolbar" aria-label="ADC 标定操作">
      <div className="industrial-calibration-command-bar">
        <button
          type="button"
          className="industrial-button industrial-button--primary industrial-calibration-command-bar__apply"
          disabled={applyBlocked}
          onClick={onApply}
        >
          <CheckCircle2 size={15} aria-hidden="true" />
          应用标定
        </button>
        <button
          type="button"
          className="industrial-button industrial-button--secondary industrial-calibration-command-bar__action"
          onClick={onExport}
        >
          <Download size={15} aria-hidden="true" />
          导出 JSON
        </button>
        <button
          type="button"
          className="industrial-button industrial-button--secondary industrial-calibration-command-bar__action"
          disabled={disabled}
          onClick={onImport}
        >
          <Upload size={15} aria-hidden="true" />
          导入 JSON
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
  const activeCount = countHeaterCurvePoints(heaterCurve.active)
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
            ['EEPROM', `${activeCount}/8`],
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
  title,
  referenceLabel,
  referenceValue,
  referenceUnit,
  draftFit,
  samples,
  disabled = false,
  onReferenceChange,
  onCapture,
  onClear,
  onManualFit,
}: {
  title: string
  referenceLabel: string
  referenceValue: number
  referenceUnit: string
  draftFit: CalibrationState['draftFit']['rtdAdc']
  samples: Array<RtdCalibrationSample | VinCalibrationSample | null>
  disabled?: boolean
  onReferenceChange: (value: number) => void
  onCapture: () => void | Promise<void>
  onClear: () => void | Promise<void>
  onManualFit: (gain: number, offsetMv: number) => void | Promise<void>
}) {
  const [manualGain, setManualGain] = useState(() => draftFit.gain.toFixed(5))
  const [manualOffsetMv, setManualOffsetMv] = useState(() => draftFit.offsetMv.toFixed(1))
  const sampleCount = samples.filter(Boolean).length

  useEffect(() => {
    setManualGain(draftFit.gain.toFixed(5))
    setManualOffsetMv(draftFit.offsetMv.toFixed(1))
  }, [draftFit.gain, draftFit.offsetMv])

  const parsedManualGain = Number(manualGain)
  const parsedManualOffsetMv = Number(manualOffsetMv)
  const manualFitInvalid =
    !Number.isFinite(parsedManualGain) ||
    parsedManualGain <= 0 ||
    !Number.isFinite(parsedManualOffsetMv)

  return (
    <>
      <div className="industrial-calibration-manual-fit">
        <label>
          <span>草稿增益</span>
          <span className="industrial-calibration-input">
            <input
              type="number"
              inputMode="decimal"
              step="0.00001"
              disabled={disabled}
              value={manualGain}
              onChange={(event) => setManualGain(event.currentTarget.value)}
            />
            <small>x</small>
          </span>
        </label>
        <label>
          <span>草稿偏移</span>
          <span className="industrial-calibration-input">
            <input
              type="number"
              inputMode="decimal"
              step="0.1"
              disabled={disabled}
              value={manualOffsetMv}
              onChange={(event) => setManualOffsetMv(event.currentTarget.value)}
            />
            <small>mV</small>
          </span>
        </label>
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
          disabled={disabled || manualFitInvalid}
          onClick={() => onManualFit(parsedManualGain, parsedManualOffsetMv)}
        >
          设置草稿拟合
        </button>
      </div>

      <div className="industrial-calibration-capture-row">
        <label>
          <span>{referenceLabel}</span>
          <span className="industrial-calibration-input">
            <input
              type="number"
              aria-label={referenceLabel}
              disabled={disabled}
              value={Number.isFinite(referenceValue) ? referenceValue : 0}
              onChange={(event) => onReferenceChange(Number(event.currentTarget.value))}
            />
            <small>{referenceUnit}</small>
          </span>
        </label>
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
          disabled={disabled}
          onClick={onCapture}
        >
          采集样本
        </button>
        <button
          type="button"
          className="industrial-button industrial-button--danger-quiet"
          disabled={disabled || sampleCount === 0}
          aria-label={`清空 ${title} 草稿样本`}
          onClick={onClear}
        >
          <Trash2 size={14} aria-hidden="true" />
          清空
        </button>
      </div>
    </>
  )
}

function CalibrationFitSummaryRow({
  label,
  fit,
}: {
  label: string
  fit: CalibrationState['activeFit']['rtdAdc']
}) {
  return (
    <div className="industrial-calibration-property-list__fit-group">
      <dt>{label}</dt>
      <dd>
        <span className="industrial-calibration-fit-chip">{calibrationFitMode(fit)}</span>
        <span>{fit.gain.toFixed(5)}x</span>
        <span>{fit.offsetMv.toFixed(1)}mV</span>
      </dd>
    </div>
  )
}

function CalibrationFitStatusSummary({
  liveLabel,
  liveValue,
  activeFit,
  draftFit,
}: {
  liveLabel: string
  liveValue: string
  activeFit: CalibrationState['activeFit']['rtdAdc']
  draftFit: CalibrationState['draftFit']['rtdAdc']
}) {
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
      <CalibrationFitSummaryRow label="当前" fit={activeFit} />
      <CalibrationFitSummaryRow label="草稿" fit={draftFit} />
    </dl>
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
  samples,
  disabled = false,
  onDelete,
}: {
  channel: CalibrationChannel
  title: string
  samples: Array<RtdCalibrationSample | VinCalibrationSample | null>
  disabled?: boolean
  onDelete: (sampleIndex: number) => void | Promise<void>
}) {
  const sampleCount = samples.filter(Boolean).length
  const sampleKeys = calibrationSampleKeys(samples)
  const isRtdChannel = channel === 'rtd_adc'
  const populatedSamples = samples
    .map((sample, index) => (sample ? { ...sample, index } : null))
    .filter((sample): sample is (RtdCalibrationSample | VinCalibrationSample) & { index: number } =>
      Boolean(sample)
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

  return (
    <section className="industrial-calibration-samples-scroll" aria-label={`${title} 样本列表`}>
      <div className="industrial-calibration-channel__header">
        <h3 className="industrial-section-title">{title}</h3>
        <span>{sampleCount}/8 个样本</span>
      </div>
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
            'industrial-calibration-samples',
            populatedSamples.length === 0 && 'industrial-calibration-samples--empty'
          )}
          aria-label={`${title} 样本`}
        >
          <thead>
            <tr>
              <th scope="col">槽位</th>
              <th scope="col">观测值</th>
              <th scope="col">目标值</th>
              <th scope="col">操作</th>
            </tr>
          </thead>
          <tbody>
            {populatedSamples.length > 0 ? (
              populatedSamples.map((sample) => (
                <tr key={sampleKeys[sample.index]}>
                  <td>#{sample.index + 1}</td>
                  <td>
                    <strong>{sample.observedMv}mV</strong>
                  </td>
                  <td>{sample.expectedMv}mV</td>
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
                </tr>
              ))
            ) : (
              <tr className="industrial-calibration-samples__placeholder-row">
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
    if (followTail) {
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
          rowVirtualizer.scrollToIndex(filteredEvents.length - 1, { align: 'end' })
        })
      }

      return next
    })
  }

  const virtualItems = rowVirtualizer.getVirtualItems()
  const latestSourceLabel = filteredEvents[0]?.source
    ? (eventSourceLabels[filteredEvents[0].source] ?? filteredEvents[0].source.toUpperCase())
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
        <span>{filteredEvents[0]?.time}</span>
        <strong>{latestSourceLabel}</strong>
        <p>{filteredEvents[0]?.message ?? '暂无追踪帧'}</p>
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
