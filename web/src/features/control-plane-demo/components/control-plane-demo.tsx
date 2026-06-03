import { useVirtualizer } from '@tanstack/react-virtual'
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
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
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
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
import type {
  CalibrationChannel,
  CalibrationConfigRequest,
  CalibrationPackage,
  CalibrationState,
} from '../contracts'
import { defaultDevdBaseUrl, type LiveDevdOptions, useLiveDevdScenario } from '../live-devd'
import {
  type LiveWebSerialControls,
  type LiveWebSerialOptions,
  useLiveWebSerialScenario,
} from '../live-web-serial'
import { controlPlaneScenario, degradedControlPlaneScenario } from '../mock-data'
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

type ConsoleView = 'dashboard' | 'settings' | 'calibration' | 'update' | 'add-device'
type FlashRunStatus = 'idle' | 'running' | 'passed' | 'flashing' | 'flashed'
type AddDeviceKind = 'wifi' | 'web-serial' | 'bridge'
type LogFilter = 'all' | EventLogEntry['tone']

interface ActionFeedback {
  title: string
  detail: string
  tone: 'info' | 'success' | 'warning'
}

const LOG_FEED_SIZE = 1000
const LOG_FEED_STEP_SECONDS = 3
const LOG_FEED_START_SECONDS = 20 * 3600 + 14 * 60 + 3
const LOG_FILTER_OPTIONS: Array<{ value: LogFilter; label: string }> = [
  { value: 'all', label: 'All' },
  { value: 'info', label: 'Info' },
  { value: 'success', label: 'Ok' },
  { value: 'warning', label: 'Warn' },
  { value: 'danger', label: 'Error' },
]
const TARGET_TEMP_MIN = 0
const TARGET_TEMP_MAX = 400
const TARGET_TEMP_STEP = 5
const PPS_STEP_MV = 100
const PPS_STEP_MA = 50
const PPS_MAX_MV = 21_000
const PRESET_COMMIT_DEBOUNCE_MS = 650
const PRESET_TEMPS_C = [50, 100, 120, 150, 180, 200, 210, 220, 250, 300]
const PRESETS_C = PRESET_TEMPS_C.map((tempC) => tempC as number | null)
const PRESET_ENABLED = PRESETS_C.map((preset) => preset != null)
const PRESET_SLOT_IDS = ['M1', 'M2', 'M3', 'M4', 'M5', 'M6', 'M7', 'M8', 'M9', 'M10']
const BLOCKED_NETWORK_STATES = new Set(['error', 'timeout'])
const ADD_DEVICE_VALUE = '__add_device__'

const severityLabels: Record<DeviceSeverity, string> = {
  nominal: 'READY',
  warning: 'CHECK',
  offline: 'OFFLINE',
}

const transportLabels: Record<TransportKind, string> = {
  http: 'HTTP',
  serial: 'SERIAL',
  devd: 'DEVD',
  mock: 'MOCK',
  wifi: 'WIFI',
  bridge: 'BRIDGE',
}

const addDeviceOptions: Array<{
  kind: AddDeviceKind
  label: string
  detail: string
}> = [
  {
    kind: 'wifi',
    label: 'WiFi',
    detail: 'Bind a future station address without marking hardware online.',
  },
  {
    kind: 'web-serial',
    label: 'Web Serial',
    detail: 'Open a browser USB serial port and probe identity, network, and status.',
  },
  {
    kind: 'bridge',
    label: 'Bridge',
    detail: 'Prepare a native devd bridge target for local hardware control.',
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
    label: 'Dashboard',
    caption: 'thermal runtime',
    icon: Gauge,
  },
  {
    id: 'settings',
    label: 'Settings',
    caption: 'heat policy',
    icon: SlidersHorizontal,
  },
  {
    id: 'calibration',
    label: 'Calibration',
    caption: 'adc trim',
    icon: Wrench,
  },
  {
    id: 'update',
    label: 'Update',
    caption: 'firmware dry-run',
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
    activeCoolingEnabled: false,
    fanState: 'OFF' as const,
    wifiRssi: null,
    capabilities: [],
    leaseState: 'none' as const,
  }

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
    Record<string, { enabled: boolean; mv: number | null; ma: number | null }>
  >({})
  const [calibrationByDevice, setCalibrationByDevice] = useState<Record<string, CalibrationState>>(
    {}
  )
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
  const [feedback, setFeedback] = useState<ActionFeedback>({
    title: allowDemoControls ? 'Runtime synced' : 'No live target',
    detail: allowDemoControls
      ? 'Thermal state is sampled from the mock device contract.'
      : 'Connect a browser Web Serial port to load live hardware state.',
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
      deviceOptions[0] ??
      activeScenario.devices[0],
    [activeScenario.devices, deviceOptions, selectedDeviceId]
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
      feedback.detail === 'Thermal state is sampled from the mock device contract.'
    ) {
      setFeedback({
        title: 'Runtime synced',
        detail: 'Thermal state is sampled from live devd firmware status.',
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
    const manualPpsMa = manualPpsOverride
      ? manualPpsOverride.ma
      : (selectedDevice.manualPpsMa ?? null)

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
      manualPpsMa,
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
    calibrationByDevice[visibleDevice.id] ?? createDefaultCalibrationState()
  const visibleCalibrationRefs = calibrationRefsByDevice[visibleDevice.id] ?? {
    rtdTempC: Number(visibleDevice.currentTempC.toFixed(1)),
    vinMv: visibleDevice.voltageMv,
  }
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
  useEffect(() => {
    if (
      visibleDevice.transport !== 'devd' ||
      !visibleDevice.leaseId ||
      !devdBaseUrl ||
      activeView !== 'calibration'
    ) {
      return
    }
    let cancelled = false
    void controlClient
      .getCalibration(devdBaseUrl, visibleDevice.id, visibleDevice.leaseId)
      .then((calibration) => {
        if (!cancelled) {
          setCalibrationByDevice((current) => ({ ...current, [visibleDevice.id]: calibration }))
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setFeedback({
            title: 'Calibration unavailable',
            detail: errorMessage(error),
            tone: 'warning',
          })
        }
      })
    return () => {
      cancelled = true
    }
  }, [
    activeView,
    controlClient,
    devdBaseUrl,
    visibleDevice.id,
    visibleDevice.leaseId,
    visibleDevice.transport,
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
        manualPpsMa?: number
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
        return false
      }

      try {
        await controlClient.configureRuntime(devdBaseUrl, visibleDevice.id, {
          leaseId: visibleDevice.leaseId,
          ...patch,
        })
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
    [controlClient, devdBaseUrl, emitEvent, visibleDevice, webSerial]
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

  const handleDeviceChange = (deviceId: string) => {
    if (deviceId === ADD_DEVICE_VALUE) {
      setActiveView('add-device')
      setFlashRun({ status: 'idle', progress: 0 })
      flashCompletionEmittedRef.current = false
      setFeedback({
        title: 'Add device',
        detail: 'Choose WiFi, Web Serial, or Bridge from the add device page.',
        tone: 'info',
      })
      return
    }

    const nextDevice = deviceOptions.find((device) => device.id === deviceId)

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
    setActiveView('add-device')
    await handleAddDevice(kind, { showPendingDashboard: false })
  }

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

  const handleManualPpsApply = async (millivolts: number, milliamps: number) => {
    const boundedMv = clampPpsMv(millivolts, visibleDevice)
    const boundedMa = clampPpsMa(milliamps, visibleDevice)
    const liveUpdated = await configureLiveRuntime(
      { manualPpsEnabled: true, manualPpsMv: boundedMv, manualPpsMa: boundedMa },
      'manual PPS update was not accepted by devd'
    )
    if (visibleDeviceIsLive && !liveUpdated) {
      return
    }
    if (!visibleDeviceIsLive) {
      setManualPpsByDevice((current) => ({
        ...current,
        [visibleDevice.id]: { enabled: true, mv: boundedMv, ma: boundedMa },
      }))
    }
    setFeedback({
      title: 'Manual PPS applied',
      detail: `${visibleDevice.alias} is requesting ${formatVolts(boundedMv)} / ${formatAmps(boundedMa)} from PPS.`,
      tone: 'warning',
    })
    emitEvent(
      'pd',
      `manual PPS set to ${formatVolts(boundedMv)} / ${formatAmps(boundedMa)}`,
      'warning'
    )
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
        [visibleDevice.id]: { enabled: false, mv: null, ma: null },
      }))
    }
    setFeedback({
      title: 'Manual PPS cleared',
      detail: `${visibleDevice.alias} is back on automatic power control.`,
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
    const nextHeld = !heaterHeldByDevice[visibleDevice.id]
    const liveUpdated = await configureLiveRuntime(
      { heaterEnabled: !nextHeld },
      'heater hold update was not accepted by devd'
    )
    if (visibleDeviceIsLive && !liveUpdated) {
      return
    }
    setHeaterHeldByDevice((current) => ({
      ...current,
      ...(visibleDeviceIsLive ? {} : { [visibleDevice.id]: nextHeld }),
    }))
    setFeedback({
      title: nextHeld ? 'Heater held' : 'Heater resumed',
      detail: nextHeld
        ? 'Heater output is forced to 0% in the mock runtime.'
        : 'Heater output follows the target temperature again.',
      tone: nextHeld ? 'warning' : 'success',
    })
    emitEvent(
      'heater',
      nextHeld ? 'heater output held at 0%' : 'heater output resumed',
      nextHeld ? 'warning' : 'success'
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

  const handleCalibrationCapture = async (channel: CalibrationChannel) => {
    const request =
      channel === 'rtd_adc'
        ? {
            op: 'capture' as const,
            channel,
            referenceTempC: visibleCalibrationRefs.rtdTempC,
          }
        : {
            op: 'capture' as const,
            channel,
            referenceVinMv: visibleCalibrationRefs.vinMv,
          }
    try {
      await updateCalibrationDraft(request)
      setFeedback({
        title: 'Calibration draft updated',
        detail: `${channelLabel(channel)} sample captured.`,
        tone: 'success',
      })
      emitEvent('calibration', `${channelLabel(channel)} sample captured`, 'success')
    } catch (error) {
      setFeedback({ title: 'Calibration failed', detail: errorMessage(error), tone: 'warning' })
    }
  }

  const handleCalibrationDelete = async (channel: CalibrationChannel, sampleIndex: number) => {
    try {
      await updateCalibrationDraft({ op: 'delete', channel, sampleIndex })
      setFeedback({
        title: 'Calibration draft updated',
        detail: `${channelLabel(channel)} sample removed.`,
        tone: 'info',
      })
    } catch (error) {
      setFeedback({ title: 'Calibration failed', detail: errorMessage(error), tone: 'warning' })
    }
  }

  const handleCalibrationClear = async (channel: CalibrationChannel) => {
    try {
      await updateCalibrationDraft({ op: 'clear', channel })
      setFeedback({
        title: 'Calibration draft updated',
        detail: `${channelLabel(channel)} draft cleared.`,
        tone: 'info',
      })
    } catch (error) {
      setFeedback({ title: 'Calibration failed', detail: errorMessage(error), tone: 'warning' })
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
        title: 'Calibration draft updated',
        detail: `${channelLabel(channel)} draft fit set to ${gain.toFixed(5)}x / ${offsetMv.toFixed(
          1
        )}mV.`,
        tone: 'success',
      })
      emitEvent('calibration', `${channelLabel(channel)} manual fit updated`, 'success')
    } catch (error) {
      setFeedback({ title: 'Calibration failed', detail: errorMessage(error), tone: 'warning' })
    }
  }

  const handleCalibrationImport = async (calibrationPackage: CalibrationPackage) => {
    try {
      await updateCalibrationDraft({ op: 'import', package: calibrationPackage })
      setFeedback({
        title: 'Calibration imported',
        detail: 'Draft samples were replaced from JSON.',
        tone: 'success',
      })
    } catch (error) {
      setFeedback({ title: 'Calibration failed', detail: errorMessage(error), tone: 'warning' })
    }
  }

  const handleCalibrationApply = async () => {
    if (visibleDevice.heaterEnabled || visibleDevice.heaterOutputPercent !== 0) {
      setFeedback({
        title: 'Apply blocked',
        detail: 'Turn the heater off before applying ADC calibration.',
        tone: 'warning',
      })
      return
    }
    try {
      if (visibleDevice.transport === 'devd' && visibleDevice.leaseId && devdBaseUrl) {
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
        title: 'Calibration applied',
        detail: 'Active ADC calibration now matches the draft.',
        tone: 'success',
      })
      emitEvent('calibration', 'ADC calibration applied', 'success')
    } catch (error) {
      setFeedback({ title: 'Apply failed', detail: errorMessage(error), tone: 'warning' })
    }
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
              <h1>Thermal bench</h1>
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
                  onClick={() => setActiveView(view.id)}
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
                calibrationRefs={visibleCalibrationRefs}
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
          ? `${template.message} · frame ${String(index + 1).padStart(4, '0')}`
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

function cloneCalibrationPackage(calibrationPackage: CalibrationPackage): CalibrationPackage {
  return {
    rtdAdc: calibrationPackage.rtdAdc.map((sample) => (sample ? { ...sample } : null)),
    vinAdc: calibrationPackage.vinAdc.map((sample) => (sample ? { ...sample } : null)),
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
  samples: Array<{ observedMv: number; expectedMv: number } | null>,
  channel: CalibrationChannel
) {
  const custom = samples.filter((sample): sample is { observedMv: number; expectedMv: number } =>
    Boolean(sample)
  )
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

function calibrationSampleKeys(samples: Array<{ observedMv: number; expectedMv: number } | null>) {
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
    throw new Error('Calibration channel is required.')
  }
  const samples = channel === 'rtd_adc' ? draft.rtdAdc : draft.vinAdc
  if (request.op === 'clear') {
    samples.fill(null)
  } else if (request.op === 'delete') {
    if (request.sampleIndex == null || !samples[request.sampleIndex]) {
      throw new Error('Calibration sample was not found.')
    }
    samples[request.sampleIndex] = null
  } else if (request.op === 'capture') {
    const slot = samples.findIndex((sample) => sample == null)
    if (slot < 0) {
      throw new Error('Calibration channel already has 8 samples.')
    }
    samples[slot] = {
      observedMv: request.observedMv ?? (channel === 'rtd_adc' ? 1120 : 1670),
      expectedMv:
        request.expectedMv ??
        (channel === 'rtd_adc'
          ? rtdAdcMvForTemperature(request.referenceTempC ?? 0)
          : vinAdcMvForInput(request.referenceVinMv ?? 0)),
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
    throw new Error('Manual calibration fit requires a positive gain and finite offset.')
  }

  const low = Math.max(0, Math.ceil(offsetMv < 0 ? (-offsetMv + 1) / gain : 0))
  const high = Math.min(65_535, Math.floor((65_535 - offsetMv) / gain))
  if (high <= low) {
    throw new Error('Manual calibration fit is outside the ADC millivolt range.')
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
    throw new Error('Manual calibration fit is outside the ADC millivolt range.')
  }

  return points
}

function normalizeCalibrationPackage(calibrationPackage: CalibrationPackage): CalibrationPackage {
  const normalize = (samples: Array<{ observedMv: number; expectedMv: number } | null>) => {
    const compacted = samples.filter(Boolean) as Array<{ observedMv: number; expectedMv: number }>
    return Array.from({ length: 8 }, (_, index) => compacted[index] ?? null)
  }
  return {
    rtdAdc: normalize(calibrationPackage.rtdAdc),
    vinAdc: normalize(calibrationPackage.vinAdc),
  }
}

function rtdAdcMvForTemperature(tempC: number) {
  const resistance =
    tempC >= 0
      ? 1000 * (1 + 3.9083e-3 * tempC - 5.775e-7 * tempC * tempC)
      : 1000 *
        (1 +
          3.9083e-3 * tempC -
          5.775e-7 * tempC * tempC -
          4.183e-12 * (tempC - 100) * tempC * tempC * tempC)
  return Math.round((3000 * resistance) / (2490 + resistance))
}

function vinAdcMvForInput(inputMv: number) {
  return Math.round((inputMv * 5100) / (56_000 + 5100))
}

function channelLabel(channel: CalibrationChannel) {
  return channel === 'rtd_adc' ? 'RTD ADC' : 'VIN ADC'
}

function calibrationFitMode(fit: CalibrationState['activeFit']['rtdAdc']) {
  if (fit.customSampleCount >= 2) {
    return 'Custom'
  }
  if (fit.customSampleCount === 1) {
    return '1-point'
  }
  return 'Default'
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : 'Request failed.'
}

function deviceControlBlockReason(device: DeviceTarget) {
  if (device.severity === 'offline') {
    return 'Target is offline.'
  }

  const networkState = device.networkState
  if (networkState && BLOCKED_NETWORK_STATES.has(networkState)) {
    return device.transportIssue ?? 'Device control is blocked until the transport recovers.'
  }

  return null
}

function isNoLiveTargetDevice(device: DeviceTarget) {
  return device.id === NO_LIVE_TARGET_ID
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
  const minMv = device.ppsCapabilityMinMv ?? 0
  const maxMv = Math.min(device.ppsCapabilityMaxMv ?? 0, PPS_MAX_MV)
  if (minMv <= 0 || maxMv < minMv) {
    return null
  }
  return { minMv, maxMv }
}

function clampPpsMv(value: number, device: DeviceTarget) {
  const range = ppsCapabilityRange(device)
  const minMv = range?.minMv ?? PPS_STEP_MV
  const maxMv = range?.maxMv ?? PPS_MAX_MV
  const rounded = Math.round(value / PPS_STEP_MV) * PPS_STEP_MV
  return Math.min(maxMv, Math.max(minMv, rounded))
}

function clampPpsMa(value: number, device: DeviceTarget) {
  const maxMa = device.ppsCapabilityMaxMa ?? 3_000
  const rounded = Math.round(value / PPS_STEP_MA) * PPS_STEP_MA
  return Math.min(maxMa, Math.max(PPS_STEP_MA, rounded))
}

function defaultManualPpsMv(device: DeviceTarget) {
  return clampPpsMv(
    device.manualPpsMv ?? device.pdContractMv ?? device.ppsCapabilityMinMv ?? 12_000,
    device
  )
}

function defaultManualPpsMa(device: DeviceTarget) {
  return clampPpsMa(
    device.manualPpsMa ?? device.ppsCapabilityMaxMa ?? device.currentMa ?? 3_000,
    device
  )
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

function formatPdContract(millivolts: number, milliamps: number) {
  const volts = formatVolts(millivolts)
  const amps = formatAmps(milliamps)

  return amps === 'N/A' ? volts : `${volts} / ${amps}`
}

function pdStateLabel(state: DeviceTarget['pdState']) {
  const labels: Record<DeviceTarget['pdState'], string> = {
    negotiating: 'negotiating',
    ready: 'ready',
    fallback_5v: 'fallback',
    fault: 'fault',
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
    <section className="industrial-status-strip" aria-label="Current target">
      <div className="industrial-target-picker">
        <Select value={device.id} onValueChange={onDeviceChange}>
          <SelectTrigger
            aria-label="Target"
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
              Add device
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      <StatusDatum label="Transport" value={transportLabels[device.transport]} />
      <StatusDatum label="Lease" value={device.leaseState?.toUpperCase() ?? 'N/A'} />
      <StatusDatum label="Plate" value={formatTemp(device.currentTempC)} />
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
  calibrationRefs,
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
  calibrationRefs: { rtdTempC: number; vinMv: number }
  flashRun: { status: FlashRunStatus; progress: number }
  onTargetTempChange: (nextTargetTemp: number) => void
  onPresetSlotChange: (presetIndex: number) => void | Promise<void>
  onPresetTempChange: (nextTempC: number) => void | Promise<void>
  onPresetEnabledChange: (nextEnabled: boolean) => void | Promise<void>
  onFanPolicyChange: (fanState: DeviceTarget['fanState']) => void
  onManualPpsApply: (millivolts: number, milliamps: number) => void | Promise<void>
  onManualPpsClear: () => void | Promise<void>
  onHeaterHoldToggle: () => void
  onArtifactChange: (artifactId: string) => void
  onDeviceSelect: (deviceId: string) => void
  onQuickAddDevice: (kind: AddDeviceKind) => void
  onAddDevice: (kind: AddDeviceKind) => void
  onStartDryRun: () => void
  onStartFlash: () => void
  onCalibrationReferenceChange: (channel: CalibrationChannel, value: number) => void
  onCalibrationCapture: (channel: CalibrationChannel) => void | Promise<void>
  onCalibrationDelete: (channel: CalibrationChannel, sampleIndex: number) => void | Promise<void>
  onCalibrationClear: (channel: CalibrationChannel) => void | Promise<void>
  onCalibrationManualFit: (
    channel: CalibrationChannel,
    gain: number,
    offsetMv: number
  ) => void | Promise<void>
  onCalibrationImport: (calibrationPackage: CalibrationPackage) => void | Promise<void>
  onCalibrationApply: () => void | Promise<void>
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
        refs={calibrationRefs}
        feedback={feedback}
        onReferenceChange={onCalibrationReferenceChange}
        onCapture={onCalibrationCapture}
        onDelete={onCalibrationDelete}
        onClear={onCalibrationClear}
        onManualFit={onCalibrationManualFit}
        onImport={onCalibrationImport}
        onApply={onCalibrationApply}
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
    <div className="industrial-view-panel">
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
  onManualPpsApply: (millivolts: number, milliamps: number) => void | Promise<void>
  onManualPpsClear: () => void | Promise<void>
  onHeaterHoldToggle: () => void
}) {
  const [advancedOpen, setAdvancedOpen] = useState(false)
  const manualPpsDefaultMv = defaultManualPpsMv(device)
  const manualPpsDefaultMa = defaultManualPpsMa(device)
  const [manualPpsDraftMv, setManualPpsDraftMv] = useState(() => manualPpsDefaultMv)
  const [manualPpsDraftMa, setManualPpsDraftMa] = useState(() => manualPpsDefaultMa)
  const [manualPpsDraftDirty, setManualPpsDraftDirty] = useState(false)
  const manualPpsDeviceIdRef = useRef(device.id)
  useEffect(() => {
    const deviceChanged = manualPpsDeviceIdRef.current !== device.id
    manualPpsDeviceIdRef.current = device.id
    if (!deviceChanged && advancedOpen && manualPpsDraftDirty) {
      return
    }

    setManualPpsDraftMv(manualPpsDefaultMv)
    setManualPpsDraftMa(manualPpsDefaultMa)
    setManualPpsDraftDirty(false)
  }, [advancedOpen, device.id, manualPpsDefaultMa, manualPpsDefaultMv, manualPpsDraftDirty])
  const heaterState =
    device.severity === 'offline'
      ? 'offline'
      : device.pdState === 'ready'
        ? device.heaterOutputPercent > 0
          ? 'holding'
          : 'held'
        : device.pdState
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
            value={formatPdContract(device.pdContractMv, device.currentMa)}
            detail={`${formatVolts(device.pdRequestMv)} requested / ${pdStateLabel(device.pdState)}`}
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
        valueMa={manualPpsDraftMa}
        onOpenChange={setAdvancedOpen}
        onValueChange={(millivolts) => {
          setManualPpsDraftMv(millivolts)
          setManualPpsDraftDirty(true)
        }}
        onCurrentChange={(milliamps) => {
          setManualPpsDraftMa(milliamps)
          setManualPpsDraftDirty(true)
        }}
        onApply={() => onManualPpsApply(manualPpsDraftMv, manualPpsDraftMa)}
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
          {device.heaterOutputPercent > 0 ? 'Hold heater' : 'Resume heater'}
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
  valueMa,
  onOpenChange,
  onValueChange,
  onCurrentChange,
  onApply,
  onClear,
}: {
  device: DeviceTarget
  open: boolean
  valueMv: number
  valueMa: number
  onOpenChange: (open: boolean) => void
  onValueChange: (millivolts: number) => void
  onCurrentChange: (milliamps: number) => void
  onApply: () => void | Promise<void>
  onClear: () => void | Promise<void>
}) {
  const range = ppsCapabilityRange(device)
  const maxMa = device.ppsCapabilityMaxMa ?? null
  const disabled = device.severity === 'offline' || !range || maxMa == null
  const clearDisabled = device.severity === 'offline' || !device.manualPpsEnabled
  const capabilityText = range
    ? `${formatVolts(range.minMv)}-${formatVolts(range.maxMv)} / ${maxMa ? formatAmps(maxMa) : 'current unknown'} source range`
    : 'No PPS APDO reported'
  const statusText = device.manualPpsEnabled
    ? `Manual ${formatVolts(device.manualPpsMv ?? valueMv)} / ${formatAmps(device.manualPpsMa ?? valueMa)}`
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
            <strong>
              {formatVolts(valueMv)} / {formatAmps(valueMa)}
            </strong>
            <span>
              {device.manualPpsEnabled ? 'Manual override armed' : 'Automatic control active'}
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
              max={range?.maxMv ?? PPS_MAX_MV}
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
          <div className="industrial-pps-control">
            <label htmlFor="manual-pps-current-slider">
              <span>Current request</span>
              <strong>{formatAmps(valueMa)}</strong>
            </label>
            <input
              id="manual-pps-current-slider"
              type="range"
              min={PPS_STEP_MA}
              max={maxMa ?? 3_000}
              step={PPS_STEP_MA}
              value={valueMa}
              disabled={disabled}
              aria-label="Manual PPS current"
              onChange={(event) => onCurrentChange(Number(event.currentTarget.value))}
            />
            <div className="industrial-pps-control__bounds">
              <span>{formatAmps(PPS_STEP_MA)}</span>
              <span>{maxMa ? formatAmps(maxMa) : 'Unavailable'}</span>
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
              Manual PPS pauses automatic voltage requests. Current is a requested value validated
              against the advertised APDO capability.
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
  refs,
  feedback,
  onReferenceChange,
  onCapture,
  onDelete,
  onClear,
  onManualFit,
  onImport,
  onApply,
}: {
  device: DeviceTarget
  calibration: CalibrationState
  refs: { rtdTempC: number; vinMv: number }
  feedback: ActionFeedback
  onReferenceChange: (channel: CalibrationChannel, value: number) => void
  onCapture: (channel: CalibrationChannel) => void | Promise<void>
  onDelete: (channel: CalibrationChannel, sampleIndex: number) => void | Promise<void>
  onClear: (channel: CalibrationChannel) => void | Promise<void>
  onManualFit: (channel: CalibrationChannel, gain: number, offsetMv: number) => void | Promise<void>
  onImport: (calibrationPackage: CalibrationPackage) => void | Promise<void>
  onApply: () => void | Promise<void>
}) {
  const fileInputRef = useRef<HTMLInputElement | null>(null)
  const applyBlocked = device.heaterEnabled || device.heaterOutputPercent !== 0
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
  const rtdSampleCount = calibration.draft.rtdAdc.filter(Boolean).length
  const vinSampleCount = calibration.draft.vinAdc.filter(Boolean).length
  const draftSampleCount = rtdSampleCount + vinSampleCount
  const draftPackageDetail = `${rtdSampleCount}/8 RTD · ${vinSampleCount}/8 VIN`
  return (
    <div className="industrial-view-panel">
      <PanelHeader kicker="Calibration" title="ADC trim" />
      <div className="industrial-calibration-workbench">
        <section className="industrial-calibration-topbar" aria-label="Calibration package">
          <div className="industrial-calibration-summary industrial-calibration-summary--topbar">
            <CalibrationSummaryCard
              label="Live RTD"
              value={formatTemp(device.currentTempC)}
              detail={`${calibrationFitMode(calibration.activeFit.rtdAdc)} fit`}
            />
            <CalibrationSummaryCard
              label="Live VIN"
              value={formatVolts(device.voltageMv)}
              detail={`${calibrationFitMode(calibration.activeFit.vinAdc)} fit`}
            />
            <CalibrationSummaryCard
              label="Draft package"
              value={`${draftSampleCount}/16`}
              detail={draftPackageDetail}
              tone={draftSampleCount === 0 ? 'muted' : 'accent'}
            />
          </div>
          <div className="industrial-calibration-command-bar">
            <button
              type="button"
              className="industrial-button industrial-button--primary industrial-calibration-command-bar__apply"
              disabled={applyBlocked}
              onClick={onApply}
            >
              <CheckCircle2 size={15} aria-hidden="true" />
              Apply calibration
            </button>
            <button
              type="button"
              className="industrial-button industrial-button--secondary industrial-calibration-command-bar__action"
              onClick={exportCalibration}
            >
              <Download size={15} aria-hidden="true" />
              Export JSON
            </button>
            <button
              type="button"
              className="industrial-button industrial-button--secondary industrial-calibration-command-bar__action"
              onClick={() => fileInputRef.current?.click()}
            >
              <Upload size={15} aria-hidden="true" />
              Import JSON
            </button>
            <input
              ref={fileInputRef}
              type="file"
              accept="application/json,.json"
              hidden
              onChange={(event) => void importFile(event.currentTarget.files?.[0] ?? null)}
            />
          </div>
          <ActionFeedbackPanel feedback={feedback} compact />
        </section>
        <div className="industrial-calibration-grid">
          <CalibrationChannelPanel
            channel="rtd_adc"
            title="RTD ADC"
            referenceLabel="Reference temperature"
            referenceValue={refs.rtdTempC}
            referenceUnit="C"
            activeFit={calibration.activeFit.rtdAdc}
            draftFit={calibration.draftFit.rtdAdc}
            samples={calibration.draft.rtdAdc}
            onReferenceChange={(value) => onReferenceChange('rtd_adc', value)}
            onCapture={() => onCapture('rtd_adc')}
            onDelete={(sampleIndex) => onDelete('rtd_adc', sampleIndex)}
            onClear={() => onClear('rtd_adc')}
            onManualFit={(gain, offsetMv) => onManualFit('rtd_adc', gain, offsetMv)}
          />

          <CalibrationChannelPanel
            channel="vin_adc"
            title="VIN ADC"
            referenceLabel="Reference VIN"
            referenceValue={refs.vinMv}
            referenceUnit="mV"
            activeFit={calibration.activeFit.vinAdc}
            draftFit={calibration.draftFit.vinAdc}
            samples={calibration.draft.vinAdc}
            onReferenceChange={(value) => onReferenceChange('vin_adc', value)}
            onCapture={() => onCapture('vin_adc')}
            onDelete={(sampleIndex) => onDelete('vin_adc', sampleIndex)}
            onClear={() => onClear('vin_adc')}
            onManualFit={(gain, offsetMv) => onManualFit('vin_adc', gain, offsetMv)}
          />
        </div>
      </div>
    </div>
  )
}

function CalibrationChannelPanel({
  title,
  referenceLabel,
  referenceValue,
  referenceUnit,
  activeFit,
  draftFit,
  samples,
  onReferenceChange,
  onCapture,
  onDelete,
  onClear,
  onManualFit,
}: {
  channel: CalibrationChannel
  title: string
  referenceLabel: string
  referenceValue: number
  referenceUnit: string
  activeFit: CalibrationState['activeFit']['rtdAdc']
  draftFit: CalibrationState['draftFit']['rtdAdc']
  samples: Array<{ observedMv: number; expectedMv: number } | null>
  onReferenceChange: (value: number) => void
  onCapture: () => void | Promise<void>
  onDelete: (sampleIndex: number) => void | Promise<void>
  onClear: () => void | Promise<void>
  onManualFit: (gain: number, offsetMv: number) => void | Promise<void>
}) {
  const [manualGain, setManualGain] = useState(() => draftFit.gain.toFixed(5))
  const [manualOffsetMv, setManualOffsetMv] = useState(() => draftFit.offsetMv.toFixed(1))
  const sampleCount = samples.filter(Boolean).length
  const sampleKeys = calibrationSampleKeys(samples)
  const populatedSamples = samples
    .map((sample, index) => (sample ? { ...sample, index } : null))
    .filter((sample): sample is { observedMv: number; expectedMv: number; index: number } =>
      Boolean(sample)
    )
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
    <section className="industrial-calibration-channel">
      <div className="industrial-calibration-channel__header">
        <h3 className="industrial-section-title">{title}</h3>
        <span>{sampleCount}/8 samples</span>
      </div>

      <table className="industrial-calibration-fit-table" aria-label={`${title} fit summary`}>
        <thead>
          <tr>
            <th scope="col">Fit</th>
            <th scope="col">Mode</th>
            <th scope="col">Gain</th>
            <th scope="col">Offset</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <th scope="row">Active</th>
            <td>{calibrationFitMode(activeFit)}</td>
            <td>
              <strong>{activeFit.gain.toFixed(5)}x</strong>
            </td>
            <td>{activeFit.offsetMv.toFixed(1)}mV</td>
          </tr>
          <tr>
            <th scope="row">Draft</th>
            <td>{calibrationFitMode(draftFit)}</td>
            <td>
              <strong>{draftFit.gain.toFixed(5)}x</strong>
            </td>
            <td>{draftFit.offsetMv.toFixed(1)}mV</td>
          </tr>
        </tbody>
      </table>

      <div className="industrial-calibration-manual-fit">
        <label>
          <span>Draft gain</span>
          <span className="industrial-calibration-input">
            <input
              type="number"
              inputMode="decimal"
              step="0.00001"
              value={manualGain}
              onChange={(event) => setManualGain(event.currentTarget.value)}
            />
            <small>x</small>
          </span>
        </label>
        <label>
          <span>Draft offset</span>
          <span className="industrial-calibration-input">
            <input
              type="number"
              inputMode="decimal"
              step="0.1"
              value={manualOffsetMv}
              onChange={(event) => setManualOffsetMv(event.currentTarget.value)}
            />
            <small>mV</small>
          </span>
        </label>
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
          disabled={manualFitInvalid}
          onClick={() => onManualFit(parsedManualGain, parsedManualOffsetMv)}
        >
          Set draft fit
        </button>
      </div>

      <div className="industrial-calibration-capture-row">
        <label>
          <span>{referenceLabel}</span>
          <span className="industrial-calibration-input">
            <input
              type="number"
              value={Number.isFinite(referenceValue) ? referenceValue : 0}
              onChange={(event) => onReferenceChange(Number(event.currentTarget.value))}
            />
            <small>{referenceUnit}</small>
          </span>
        </label>
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
          onClick={onCapture}
        >
          Capture sample
        </button>
        <button
          type="button"
          className="industrial-button industrial-button--danger-quiet"
          disabled={sampleCount === 0}
          aria-label={`Clear ${title} draft samples`}
          onClick={onClear}
        >
          <Trash2 size={14} aria-hidden="true" />
          Clear
        </button>
      </div>
      {populatedSamples.length > 0 ? (
        <section
          className="industrial-calibration-samples-scroll"
          aria-label={`${title} sample list`}
        >
          <table className="industrial-calibration-samples" aria-label={`${title} samples`}>
            <thead>
              <tr>
                <th scope="col">Slot</th>
                <th scope="col">Observed</th>
                <th scope="col">Expected</th>
                <th scope="col">Action</th>
              </tr>
            </thead>
            <tbody>
              {populatedSamples.map((sample) => (
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
                      aria-label={`Delete ${title} sample ${sample.index + 1}`}
                      onClick={() => onDelete(sample.index)}
                    >
                      <Trash2 size={14} aria-hidden="true" />
                      Delete
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      ) : (
        <p className="industrial-calibration-empty">
          <span>Capture with physical reference</span>
          <small>8 draft slots available</small>
        </p>
      )}
    </section>
  )
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

  return (
    <aside className="industrial-panel industrial-log-panel" aria-label="Global log">
      <div className="industrial-log-panel__header">
        <div>
          <p className="industrial-label text-[#a8b2d1]">Global log</p>
          <h2>Runtime trace</h2>
        </div>
        <fieldset className="industrial-log-filters">
          <legend className="sr-only">Log level filter</legend>
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
        <strong>{filteredEvents[0]?.source ?? 'trace'}</strong>
        <p>{filteredEvents[0]?.message ?? 'No trace frames'}</p>
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
          {followTail ? 'Following tail' : 'Follow tail'}
        </button>
        <div className="industrial-log-count" aria-live="polite">
          {filteredEvents.length} / {events.length} frames
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
                <strong>{event.source}</strong>
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
      <p className="industrial-label">Last action</p>
      <strong>{feedback.title}</strong>
      <span>{feedback.detail}</span>
    </div>
  )
}

function CalibrationSummaryCard({
  label,
  value,
  detail,
  tone = 'default',
}: {
  label: string
  value: string
  detail: string
  tone?: 'default' | 'accent' | 'muted'
}) {
  return (
    <div className={`industrial-calibration-summary-card is-${tone}`}>
      <p className="industrial-label">{label}</p>
      <strong>{value}</strong>
      <span>{detail}</span>
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
