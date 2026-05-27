import { useVirtualizer } from '@tanstack/react-virtual'
import {
  CheckCircle2,
  Fan,
  Gauge,
  Minus,
  Plus,
  Power,
  RefreshCw,
  SlidersHorizontal,
  ToggleRight,
  Upload,
  Zap,
} from 'lucide-react'
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import SimpleBar from 'simplebar-react'
import { Switch } from '@/components/ui/switch'
import { defaultDevdBaseUrl, type LiveDevdOptions, useLiveDevdScenario } from '../live-devd'
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

interface ControlPlaneDemoProps {
  scenario?: ControlPlaneScenario
  initialView?: ConsoleView
  devd?: LiveDevdOptions
}

type ConsoleView = 'dashboard' | 'settings' | 'update'
type FlashRunStatus = 'idle' | 'running' | 'passed' | 'flashing' | 'flashed'

interface ActionFeedback {
  title: string
  detail: string
  tone: 'info' | 'success' | 'warning'
}

const LOG_FEED_SIZE = 1000
const LOG_FEED_STEP_SECONDS = 3
const LOG_FEED_START_SECONDS = 20 * 3600 + 14 * 60 + 3
const TARGET_TEMP_MIN = 30
const TARGET_TEMP_MAX = 380
const TARGET_TEMP_STEP = 5
const PRESET_COMMIT_DEBOUNCE_MS = 650
const PRESET_TEMPS_C = [50, 100, 120, 150, 180, 200, 210, 220, 250, 300]
const PRESET_ENABLED = [true, true, false, true, true, true, true, true, true, false]
const PRESET_SLOT_IDS = ['M1', 'M2', 'M3', 'M4', 'M5', 'M6', 'M7', 'M8', 'M9', 'M10']
const BLOCKED_NETWORK_STATES = new Set(['error', 'timeout'])

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
}

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
    id: 'update',
    label: 'Update',
    caption: 'firmware dry-run',
    icon: Upload,
  },
]

export function ControlPlaneDemo({
  scenario = controlPlaneScenario,
  initialView = 'dashboard',
  devd,
}: ControlPlaneDemoProps) {
  const liveScenario = useLiveDevdScenario(scenario, devd)
  const controlClient = useMemo(
    () => devd?.httpClient ?? createControlPlaneHttpClient(),
    [devd?.httpClient]
  )
  const devdBaseUrl = devd?.devdBaseUrl ?? defaultDevdBaseUrl()
  const [selectedDeviceId, setSelectedDeviceId] = useState(scenario.selectedDeviceId)
  const [activeView, setActiveView] = useState<ConsoleView>(initialView)
  const [showDegraded, setShowDegraded] = useState(false)
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
  const [artifactByDevice, setArtifactByDevice] = useState<Record<string, string>>({})
  const [flashRun, setFlashRun] = useState<{ status: FlashRunStatus; progress: number }>({
    status: 'idle',
    progress: 0,
  })
  const flashCompletionEmittedRef = useRef(false)
  const actionClockRef = useRef(LOG_FEED_START_SECONDS + 60)
  const [actionEvents, setActionEvents] = useState<EventLogEntry[]>([])
  const [feedback, setFeedback] = useState<ActionFeedback>({
    title: 'Runtime synced',
    detail: 'Thermal state is sampled from the mock device contract.',
    tone: 'info',
  })
  const activeScenario = showDegraded ? degradedControlPlaneScenario : liveScenario

  useEffect(() => {
    if (activeScenario.events.length < 2) {
      return
    }

    const timer = window.setInterval(() => {
      setStreamTick((tick) => tick + 1)
    }, 2200)

    return () => window.clearInterval(timer)
  }, [activeScenario.events.length])

  const selectedDevice = useMemo(
    () =>
      activeScenario.devices.find((device) => device.id === selectedDeviceId) ??
      activeScenario.devices[0],
    [activeScenario.devices, selectedDeviceId]
  )

  useEffect(() => {
    if (!activeScenario.devices.some((device) => device.id === selectedDeviceId)) {
      setSelectedDeviceId(activeScenario.selectedDeviceId)
      return
    }

    if (
      selectedDeviceId === scenario.selectedDeviceId &&
      activeScenario.selectedDeviceId !== scenario.selectedDeviceId
    ) {
      setSelectedDeviceId(activeScenario.selectedDeviceId)
    }
  }, [
    activeScenario.devices,
    activeScenario.selectedDeviceId,
    scenario.selectedDeviceId,
    selectedDeviceId,
  ])

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
    setTargetTempByDevice((current) => {
      let next = current
      for (const device of activeScenario.devices) {
        if (device.transport !== 'devd' || current[device.id] !== device.targetTempC) {
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
    setFanPolicyByDevice((current) => {
      let next = current
      for (const device of activeScenario.devices) {
        if (device.transport !== 'devd' || current[device.id] !== device.fanState) {
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

    const isLiveDevdDevice = selectedDevice.transport === 'devd'
    const currentTempC = isLiveDevdDevice
      ? selectedDevice.currentTempC
      : (currentTempByDevice[selectedDevice.id] ?? selectedDevice.currentTempC)
    const targetTempC = targetTempByDevice[selectedDevice.id] ?? selectedDevice.targetTempC
    const fanState = fanPolicyByDevice[selectedDevice.id] ?? selectedDevice.fanState
    const heaterOutputPercent = isLiveDevdDevice
      ? selectedDevice.heaterOutputPercent
      : Math.min(
          100,
          Math.max(
            0,
            selectedDevice.heaterOutputPercent + Math.round((targetTempC - currentTempC) / 8)
          )
        )

    return {
      ...selectedDevice,
      currentTempC,
      targetTempC,
      fanState,
      activeCoolingEnabled: selectedDevice.activeCoolingEnabled,
      heaterOutputPercent: heaterHeldByDevice[selectedDevice.id] ? 0 : heaterOutputPercent,
      wifiRssi: selectedDevice.wifiRssi,
      networkState: selectedDevice.networkState,
    }
  }, [
    activeScenario.devices,
    currentTempByDevice,
    fanPolicyByDevice,
    heaterHeldByDevice,
    selectedDevice,
    targetTempByDevice,
  ])
  const selectedPresetIndex = selectedPresetByDevice[visibleDevice.id] ?? 3
  const visiblePresetTemps = presetTempsByDevice[visibleDevice.id] ?? PRESET_TEMPS_C
  const visiblePresetEnabled = presetEnabledByDevice[visibleDevice.id] ?? PRESET_ENABLED
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
  const liveEvents = useMemo(
    () => createLiveEventFeed(activeScenario.events, streamTick),
    [activeScenario.events, streamTick]
  )
  const visibleEvents = useMemo(
    () => [...actionEvents, ...liveEvents].slice(0, LOG_FEED_SIZE),
    [actionEvents, liveEvents]
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
        activeCoolingEnabled?: boolean
        heaterEnabled?: boolean
      },
      failureMessage: string
    ) => {
      if (visibleDevice.transport !== 'devd' || !visibleDevice.leaseId || !devdBaseUrl) {
        return false
      }

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
    [controlClient, devdBaseUrl, emitEvent, visibleDevice]
  )

  useEffect(() => {
    const timer = window.setInterval(() => {
      setCurrentTempByDevice((current) => {
        if (!selectedDevice || visibleDevice.severity === 'offline') {
          return current
        }
        if (selectedDevice.transport === 'devd') {
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
    const nextDevice = activeScenario.devices.find((device) => device.id === deviceId)

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

  const handleToggleDegraded = () => {
    const nextDegraded = !showDegraded
    setShowDegraded(nextDegraded)
    setFlashRun({ status: 'idle', progress: 0 })
    flashCompletionEmittedRef.current = false
    setActionEvents([])
    setFeedback({
      title: nextDegraded ? 'Degraded mock enabled' : 'Nominal mock restored',
      detail: nextDegraded
        ? 'Transport warnings and blocked checks are now visible.'
        : 'Runtime state returned to the nominal fixture.',
      tone: nextDegraded ? 'warning' : 'success',
    })
  }

  const handleTargetTempChange = async (nextTargetTemp: number) => {
    const clampedTarget = clampTargetTemp(nextTargetTemp)
    const liveUpdated = await configureLiveRuntime(
      { targetTempC: clampedTarget },
      'target temperature update was not accepted by devd'
    )
    if (visibleDevice.transport === 'devd' && !liveUpdated) {
      return
    }
    setTargetTempByDevice((current) => ({
      ...current,
      [visibleDevice.id]: clampedTarget,
    }))
    setFeedback({
      title: 'Target updated',
      detail: `${visibleDevice.alias} target is now ${formatTemp(clampedTarget)}.`,
      tone: 'success',
    })
    emitEvent('thermal', `target temperature updated to ${formatTemp(clampedTarget)}`, 'success')
  }

  const handleFanPolicyChange = async (fanState: DeviceTarget['fanState']) => {
    const liveUpdated = await configureLiveRuntime(
      { activeCoolingEnabled: fanState !== 'OFF' },
      'fan policy update was not accepted by devd'
    )
    if (visibleDevice.transport === 'devd' && !liveUpdated) {
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

  const handlePresetSlotChange = (presetIndex: number) => {
    const presetIsEnabled = visiblePresetEnabled[presetIndex] ?? true
    setSelectedPresetByDevice((current) => ({ ...current, [visibleDevice.id]: presetIndex }))
    setFeedback({
      title: `Preset M${presetIndex + 1} selected`,
      detail: presetIsEnabled
        ? `${formatTemp(visiblePresetTemps[presetIndex])} is ready for ${visibleDevice.alias}.`
        : `${formatTemp(visiblePresetTemps[presetIndex])} is stored but disabled.`,
      tone: presetIsEnabled ? 'info' : 'warning',
    })
    emitEvent('preset', `selected M${presetIndex + 1}`, 'info')
  }

  const handlePresetTempChange = (nextTempC: number) => {
    const clampedTemp = clampTargetTemp(nextTempC)
    setPresetTempsByDevice((current) => {
      const nextTemps = [...(current[visibleDevice.id] ?? PRESET_TEMPS_C)]
      nextTemps[selectedPresetIndex] = clampedTemp

      return { ...current, [visibleDevice.id]: nextTemps }
    })
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

  const handlePresetEnabledChange = (nextEnabled: boolean) => {
    setPresetEnabledByDevice((current) => {
      const nextEnabledState = [...(current[visibleDevice.id] ?? PRESET_ENABLED)]
      nextEnabledState[selectedPresetIndex] = nextEnabled

      return { ...current, [visibleDevice.id]: nextEnabledState }
    })
    setFeedback({
      title: `Preset M${selectedPresetIndex + 1} ${nextEnabled ? 'enabled' : 'disabled'}`,
      detail: nextEnabled
        ? `${formatTemp(visiblePresetTemps[selectedPresetIndex])} can be used as a live target.`
        : 'This preset stays stored but is hidden from quick target use.',
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
    if (visibleDevice.transport === 'devd' && !liveUpdated) {
      return
    }
    setHeaterHeldByDevice((current) => ({
      ...current,
      ...(visibleDevice.transport === 'devd' ? {} : { [visibleDevice.id]: nextHeld }),
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
              devices={activeScenario.devices}
              device={visibleDevice}
              showDegraded={showDegraded}
              onDeviceChange={handleDeviceChange}
              onToggleDegraded={handleToggleDegraded}
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

          <div className="industrial-console__workspace">
            <section className="industrial-panel industrial-console__main">
              <ViewPanel
                view={activeView}
                device={visibleDevice}
                selectedPresetIndex={selectedPresetIndex}
                presetTemps={visiblePresetTemps}
                presetEnabled={visiblePresetEnabled}
                flashPhases={visibleFlashPhases}
                artifacts={activeScenario.artifacts}
                artifact={selectedArtifact}
                feedback={feedback}
                flashRun={flashRun}
                onTargetTempChange={handleTargetTempChange}
                onPresetSlotChange={handlePresetSlotChange}
                onPresetTempChange={handlePresetTempChange}
                onPresetEnabledChange={handlePresetEnabledChange}
                onFanPolicyChange={handleFanPolicyChange}
                onHeaterHoldToggle={handleHeaterHoldToggle}
                onArtifactChange={handleArtifactChange}
                onStartDryRun={handleStartDryRun}
                onStartFlash={handleStartFlash}
              />
            </section>

            <GlobalLogPanel
              events={visibleEvents}
              quiet={activeView !== 'dashboard'}
              currentView={activeView}
            />
          </div>
        </section>
      </div>
    </main>
  )
}

function createLiveEventFeed(events: EventLogEntry[], tick: number) {
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

function formatTemp(value: number) {
  if (value <= 0) {
    return 'N/A'
  }

  return `${formatTempNumber(value)}℃`
}

function formatTempNumber(value: number) {
  return value.toFixed(1).replace(/\.0$/, '')
}

function clampTargetTemp(value: number) {
  return Math.min(TARGET_TEMP_MAX, Math.max(TARGET_TEMP_MIN, Math.round(value)))
}

function formatVolts(millivolts: number) {
  if (millivolts <= 0) {
    return 'N/A'
  }

  return `${(millivolts / 1000).toFixed(millivolts % 1000 === 0 ? 0 : 1)}V`
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

function DeviceToolbar({
  devices,
  device,
  showDegraded,
  onDeviceChange,
  onToggleDegraded,
}: {
  devices: DeviceTarget[]
  device: DeviceTarget
  showDegraded: boolean
  onDeviceChange: (deviceId: string) => void
  onToggleDegraded: () => void
}) {
  return (
    <section className="industrial-status-strip" aria-label="Current target">
      <div className="industrial-target-picker">
        <label htmlFor="control-plane-target" className="sr-only">
          Target
        </label>
        <select
          id="control-plane-target"
          name="controlPlaneTarget"
          className="industrial-device-select"
          value={device.id}
          onChange={(event) => onDeviceChange(event.target.value)}
        >
          {devices.map((item) => (
            <option key={item.id} value={item.id}>
              {item.alias} / {transportLabels[item.transport]}
            </option>
          ))}
        </select>
      </div>

      <StatusDatum label="Transport" value={transportLabels[device.transport]} />
      <StatusDatum label="Lease" value={device.leaseState?.toUpperCase() ?? 'N/A'} />
      <StatusDatum label="Plate" value={formatTemp(device.currentTempC)} />
      <StatusDatum label="PD" value={formatVolts(device.pdContractMv)} />

      <button
        type="button"
        className="industrial-button industrial-button--secondary industrial-status-strip__button"
        onClick={onToggleDegraded}
      >
        <RefreshCw size={16} aria-hidden="true" />
        {showDegraded ? 'Nominal' : 'Degrade'}
      </button>
    </section>
  )
}

function ViewPanel({
  view,
  device,
  selectedPresetIndex,
  presetTemps,
  presetEnabled,
  flashPhases,
  artifacts,
  artifact,
  feedback,
  flashRun,
  onTargetTempChange,
  onPresetSlotChange,
  onPresetTempChange,
  onPresetEnabledChange,
  onFanPolicyChange,
  onHeaterHoldToggle,
  onArtifactChange,
  onStartDryRun,
  onStartFlash,
}: {
  view: ConsoleView
  device: DeviceTarget
  selectedPresetIndex: number
  presetTemps: number[]
  presetEnabled: boolean[]
  flashPhases: WorkflowPhase[]
  artifacts: FirmwareArtifact[]
  artifact?: FirmwareArtifact
  feedback: ActionFeedback
  flashRun: { status: FlashRunStatus; progress: number }
  onTargetTempChange: (nextTargetTemp: number) => void
  onPresetSlotChange: (presetIndex: number) => void
  onPresetTempChange: (nextTempC: number) => void
  onPresetEnabledChange: (nextEnabled: boolean) => void
  onFanPolicyChange: (fanState: DeviceTarget['fanState']) => void
  onHeaterHoldToggle: () => void
  onArtifactChange: (artifactId: string) => void
  onStartDryRun: () => void
  onStartFlash: () => void
}) {
  if (view === 'settings') {
    return (
      <SettingsView
        device={device}
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

  return (
    <DashboardView
      device={device}
      artifact={artifact}
      feedback={feedback}
      onTargetTempChange={onTargetTempChange}
      onHeaterHoldToggle={onHeaterHoldToggle}
    />
  )
}

function DashboardView({
  device,
  artifact,
  feedback,
  onTargetTempChange,
  onHeaterHoldToggle,
}: {
  device: DeviceTarget
  artifact?: FirmwareArtifact
  feedback: ActionFeedback
  onTargetTempChange: (nextTargetTemp: number) => void
  onHeaterHoldToggle: () => void
}) {
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
            value={formatVolts(device.pdContractMv)}
            detail={`${formatVolts(device.pdRequestMv)} requested / ${pdStateLabel(device.pdState)}`}
          />
          <StatusCard
            label="Cooling"
            value={device.fanState}
            detail={device.activeCoolingEnabled ? 'Active cooling enabled' : 'Cooling disabled'}
          />
        </div>
      </div>

      <div className="industrial-secondary-actions">
        <TargetTempControl
          value={device.targetTempC}
          disabled={device.severity === 'offline'}
          onChange={onTargetTempChange}
        />
        <button
          type="button"
          className="industrial-button industrial-button--secondary"
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
        <Zap size={14} aria-hidden="true" />
        {device.currentMa}mA
      </span>
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
  selectedPresetIndex: number
  presetTemps: number[]
  presetEnabled: boolean[]
  feedback: ActionFeedback
  onPresetSlotChange: (presetIndex: number) => void
  onPresetTempChange: (nextTempC: number) => void
  onPresetEnabledChange: (nextEnabled: boolean) => void
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
                {formatTemp(presetTemps[selectedPresetIndex])}{' '}
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
              value={device.fanState}
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
  onPresetSlotChange: (presetIndex: number) => void
  onPresetTempChange: (nextTempC: number) => void
  onPresetEnabledChange: (nextEnabled: boolean) => void
}) {
  const selectedTemp = presetTemps[selectedPresetIndex] ?? PRESET_TEMPS_C[selectedPresetIndex]
  const selectedEnabled = presetEnabled[selectedPresetIndex] ?? true
  const [draftTemp, setDraftTemp] = useState(selectedTemp)
  const draftIsDirty = clampTargetTemp(draftTemp) !== selectedTemp

  useEffect(() => {
    setDraftTemp(selectedTemp)
  }, [selectedTemp])

  useEffect(() => {
    const clampedDraftTemp = clampTargetTemp(draftTemp)

    if (!draftIsDirty) {
      return
    }

    const timer = window.setTimeout(() => {
      onPresetTempChange(clampedDraftTemp)
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
              aria-label={`${slotId} ${formatTemp(tempC)} ${isEnabled ? 'enabled' : 'disabled'}`}
              onClick={() => onPresetSlotChange(index)}
            >
              <strong>{slotId}</strong>
              <span>{formatTemp(tempC)}</span>
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
            <span>{formatTemp(selectedTemp)}</span>
          </strong>
          <small>{draftIsDirty ? 'Saving...' : 'Autosaved'}</small>
        </div>
        <TargetTempControl
          label="Preset temp"
          ariaLabel="Preset temperature"
          inputId="preset-temperature"
          inputName="presetTemperature"
          value={draftTemp}
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
              onCheckedChange={onPresetEnabledChange}
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
          <select
            className="industrial-artifact-select"
            value={artifact?.id ?? ''}
            aria-label="Firmware artifact"
            disabled={isBusy}
            onChange={(event) => onArtifactChange(event.currentTarget.value)}
          >
            {artifacts.map((item) => (
              <option key={item.id} value={item.id}>
                {item.version} · {item.profile}
              </option>
            ))}
          </select>
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

function GlobalLogPanel({
  events,
  quiet = false,
  currentView = 'dashboard',
}: {
  events: EventLogEntry[]
  quiet?: boolean
  currentView?: ConsoleView
}) {
  const scrollableNodeRef = useRef<HTMLDivElement | null>(null)
  const [followTail, setFollowTail] = useState(false)
  const rowVirtualizer = useVirtualizer({
    count: events.length,
    getScrollElement: () => scrollableNodeRef.current,
    estimateSize: () => 72,
    overscan: 8,
  })

  useLayoutEffect(() => {
    if (followTail) {
      rowVirtualizer.scrollToIndex(events.length - 1, { align: 'end' })
    }
  }, [events.length, followTail, rowVirtualizer])

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
          rowVirtualizer.scrollToIndex(events.length - 1, { align: 'end' })
        })
      }

      return next
    })
  }

  const virtualItems = rowVirtualizer.getVirtualItems()
  const latestEvent = events[0]

  return (
    <aside
      className={
        quiet
          ? 'industrial-panel industrial-log-panel is-quiet'
          : 'industrial-panel industrial-log-panel'
      }
      aria-label="Global log"
    >
      {quiet ? (
        <div className="industrial-log-quiet-card">
          <p className="industrial-label">Trace</p>
          <strong>{events.length} frames</strong>
          <span>
            {currentView === 'settings' ? 'Settings' : 'Update'} · {latestEvent?.time ?? '--:--:--'}
          </span>
        </div>
      ) : null}
      <div className="industrial-log-panel__header">
        <div>
          <p className="industrial-label text-[#a8b2d1]">Global log</p>
          <h2>Runtime trace</h2>
        </div>
      </div>
      <div className="industrial-log-panel__summary">
        <span>{events[0]?.time}</span>
        <strong>{events[0]?.source ?? 'trace'}</strong>
        <p>{events[0]?.message ?? 'No trace frames'}</p>
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
        <div
          className="industrial-log-virtual-space"
          style={{ height: `${rowVirtualizer.getTotalSize()}px` }}
        >
          {virtualItems.map((virtualItem) => {
            const event = events[virtualItem.index]

            if (!event) {
              return null
            }

            return (
              <div
                key={virtualItem.key}
                className={`industrial-event industrial-event--virtual is-${event.tone}`}
                style={{
                  height: `${virtualItem.size}px`,
                  transform: `translateY(${virtualItem.start}px)`,
                }}
              >
                <span>{event.time}</span>
                <strong>{event.source}</strong>
                <p>{event.message}</p>
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
  const options: DeviceTarget['fanState'][] = ['OFF', 'AUTO', 'RUN']

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
