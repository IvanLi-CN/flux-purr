import { useEffect, useMemo, useRef, useState } from 'react'
import type {
  ControlPlaneStatus,
  DevdDeviceRecord,
  DevdEvent,
  DevdLease,
  Identity,
  NetworkSummary,
} from './contracts'
import type { ControlPlaneHttpClient } from './transport-client'
import {
  bestEffortReleaseDevdLease,
  ControlPlaneClientError,
  createControlPlaneHttpClient,
  devdEventToLogEntry,
  devdRecordToDeviceTarget,
  isMeaningfulDevdTransportIssueEvent,
} from './transport-client'
import type { ControlPlaneScenario, DeviceTarget, EventLogEntry } from './types'

const DEVD_POLL_MS = 2_000
const DEVD_TRACE_LIMIT = 240
const DEVD_LEASE_STORAGE_PREFIX = 'flux-purr:devd-lease:'
const DEVD_DEVICE_SNAPSHOT_STORAGE_PREFIX = 'flux-purr:devd-device:'
const DEVD_DEVICE_SNAPSHOT_TTL_MS = 15_000
const DEVD_UNAVAILABLE_TARGET_ID = 'live-devd-unavailable'
const DEVD_MISSING_LIVE_TARGET_MESSAGE =
  'Authorized native serial target is temporarily unavailable; keeping the last live target until polling recovers.'
const DEVD_RECONNECTING_MESSAGE = '正在重新接管本机 devd 租约，请稍候。'
const DEVD_EVENT_KINDS = [
  'serial',
  'lease',
  'wifi',
  'runtime',
  'calibration',
  'flash',
  'transport',
] as const

export interface LiveDevdOptions {
  enabled?: boolean
  devdBaseUrl?: string | null
  httpClient?: ControlPlaneHttpClient
  includeMockDevices?: boolean
}

type DevdRefreshState = 'idle' | 'refreshing' | 'ready'

export function defaultDevdBaseUrl() {
  const env = import.meta.env as ImportMetaEnv & {
    VITE_FLUX_PURR_DEVD_URL?: string
    VITE_FLUX_PURR_ENABLE_DEVD?: string
  }
  if (env.VITE_FLUX_PURR_ENABLE_DEVD === '0') {
    return null
  }

  return env.VITE_FLUX_PURR_DEVD_URL ?? 'http://127.0.0.1:30080'
}

export function useLiveDevdScenario(
  scenario: ControlPlaneScenario,
  {
    enabled = true,
    devdBaseUrl = defaultDevdBaseUrl(),
    httpClient,
    includeMockDevices = true,
  }: LiveDevdOptions = {}
) {
  const client = useMemo(() => httpClient ?? createControlPlaneHttpClient(), [httpClient])
  const [devices, setDevices] = useState<DeviceTarget[]>(() =>
    enabled && devdBaseUrl ? readStoredLiveDevdTarget(devdBaseUrl) : []
  )
  const [refreshState, setRefreshState] = useState<DevdRefreshState>(() =>
    enabled && devdBaseUrl ? 'refreshing' : 'idle'
  )
  const [artifacts, setArtifacts] = useState<ControlPlaneScenario['artifacts']>([])
  const [recordEvents, setRecordEvents] = useState<DevdEvent[]>([])
  const [streamEvents, setStreamEvents] = useState<DevdEvent[]>([])
  const activeLeaseRef = useRef<DevdLease | null>(null)
  const activeLeaseDeviceIdRef = useRef<string | null>(null)
  const refreshInFlightRef = useRef(false)
  const liveDevdDeviceId = useMemo(
    () => devices.find((device) => device.transport === 'devd')?.id,
    [devices]
  )

  useEffect(() => {
    if (!enabled || !devdBaseUrl) {
      setDevices([])
      setRefreshState('idle')
      setArtifacts([])
      setRecordEvents([])
      setStreamEvents([])
      return
    }
    setRefreshState((current) => (current === 'ready' ? current : 'refreshing'))
    setDevices((current) => (current.length > 0 ? current : readStoredLiveDevdTarget(devdBaseUrl)))

    let cancelled = false
    const releaseActiveLease = async (mode: 'cleanup' | 'pagehide' = 'cleanup') => {
      const lease = activeLeaseRef.current
      const deviceId = activeLeaseDeviceIdRef.current
      if (!lease || !deviceId) {
        return
      }

      activeLeaseRef.current = null
      activeLeaseDeviceIdRef.current = null
      if (mode === 'pagehide') {
        releaseDevdLeaseOnPageHide({
          devdBaseUrl,
          lease,
          deviceId,
          storage: getDevdLeaseStorage(),
        })
        return
      }
      clearStoredDevdLeaseId(devdBaseUrl, deviceId)
      await client.releaseDevdLease(devdBaseUrl, lease.leaseId).catch(() => undefined)
    }

    const handlePageHide = () => {
      void releaseActiveLease('pagehide')
    }

    window.addEventListener('pagehide', handlePageHide)

    const refresh = async () => {
      if (cancelled || refreshInFlightRef.current) {
        return
      }
      refreshInFlightRef.current = true
      let records: DevdDeviceRecord[] = []
      try {
        const [nextRecords, nextArtifacts] = await Promise.all([
          client.listDevdDevices(devdBaseUrl),
          client.listDevdArtifacts(devdBaseUrl).catch(() => []),
        ])
        records = nextRecords
        if (!cancelled) {
          setArtifacts(nextArtifacts)
          setRecordEvents(devdRecordsToEvents(records))
        }
        const visibleRecords = includeMockDevices
          ? records
          : records.filter((record) => record.transport === 'native_serial')
        const baseDevices = visibleRecords.map(devdRecordToDeviceTarget)
        const liveRecord = selectLiveDevdRecord(records)
        if (!liveRecord) {
          activeLeaseRef.current = null
          activeLeaseDeviceIdRef.current = null
          if (!cancelled) {
            setDevices((current) => preserveLastLiveDevdTarget(baseDevices, current))
            setRefreshState('ready')
          }
          refreshInFlightRef.current = false
          return
        }

        if (liveRecord.connection === 'busy') {
          activeLeaseRef.current = null
          activeLeaseDeviceIdRef.current = null
          if (!cancelled) {
            setDevices(replaceDevice(baseDevices, devdRecordToReconnectingTarget(liveRecord)))
            setRefreshState('ready')
          }
          refreshInFlightRef.current = false
          return
        }

        try {
          const lease = await resolveDevdLease({
            client,
            devdBaseUrl,
            deviceId: liveRecord.id,
            currentLease: activeLeaseRef.current,
            currentLeaseDeviceId: activeLeaseDeviceIdRef.current,
            storage: getDevdLeaseStorage(),
            cancelled: () => cancelled,
          })
          activeLeaseRef.current = lease
          activeLeaseDeviceIdRef.current = liveRecord.id

          const live = await client.probeDevdDevice(devdBaseUrl, liveRecord.id, lease.leaseId)
          const liveDevice = devdRecordToDeviceTarget(mergeDevdProbeRecord(liveRecord, live))
          liveDevice.leaseState = 'active'
          liveDevice.leaseId = lease.leaseId
          writeStoredLiveDevdTarget(devdBaseUrl, liveDevice)

          if (!cancelled) {
            setDevices(replaceDevice(baseDevices, liveDevice))
            setRefreshState('ready')
          }
        } catch (error) {
          const failedLease = activeLeaseRef.current
          if (isLeaseInvalid(error)) {
            activeLeaseRef.current = null
            activeLeaseDeviceIdRef.current = null
            clearStoredDevdLeaseId(devdBaseUrl, liveRecord.id)
          }
          const issueDevice = devdRecordToDeviceTarget(liveRecord)
          issueDevice.severity = 'warning'
          issueDevice.leaseState = leaseStateForError(error, failedLease)
          issueDevice.leaseId = failedLease?.leaseId
          issueDevice.transportIssue ||= issueMessage(error)
          if (!cancelled) {
            setDevices(replaceDevice(baseDevices, issueDevice))
            setRefreshState('ready')
          }
        } finally {
          refreshInFlightRef.current = false
        }
      } catch (error) {
        if (!cancelled) {
          setDevices((current) => degradeDevicesForRefreshError(current, error))
          setRefreshState('ready')
        }
        refreshInFlightRef.current = false
      }
    }

    void refresh()
    const timer = window.setInterval(refresh, DEVD_POLL_MS)

    return () => {
      cancelled = true
      window.clearInterval(timer)
      window.removeEventListener('pagehide', handlePageHide)
      setStreamEvents([])
      void releaseActiveLease()
    }
  }, [client, devdBaseUrl, enabled, includeMockDevices])

  useEffect(() => {
    if (!enabled || !devdBaseUrl || !liveDevdDeviceId || typeof EventSource === 'undefined') {
      setStreamEvents([])
      return
    }

    const eventSource = new EventSource(
      `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(liveDevdDeviceId)}/events`
    )
    const handleEvent = (message: MessageEvent<string>) => {
      const event = parseDevdEvent(message.data)
      if (!event || event.deviceId !== liveDevdDeviceId) {
        return
      }
      setStreamEvents((current) => appendDevdEvent(current, event))
    }

    eventSource.onmessage = handleEvent
    for (const kind of DEVD_EVENT_KINDS) {
      eventSource.addEventListener(kind, handleEvent)
    }

    return () => {
      for (const kind of DEVD_EVENT_KINDS) {
        eventSource.removeEventListener(kind, handleEvent)
      }
      eventSource.close()
      setStreamEvents([])
    }
  }, [devdBaseUrl, enabled, liveDevdDeviceId])

  const devdEvents = useMemo(
    () => devdEventsToLogEntries(recordEvents, streamEvents),
    [recordEvents, streamEvents]
  )
  const latestTransportIssueByDevice = useMemo(
    () => latestDevdTransportIssueByDevice(recordEvents, streamEvents),
    [recordEvents, streamEvents]
  )

  return useMemo(() => {
    if (devices.length === 0 && refreshState === 'refreshing') {
      return createBootstrappingLiveDevdScenario(scenario)
    }

    if (devices.length === 0) {
      return scenario
    }

    const liveDevices = prioritizeLiveDevdDevices(devices).map((device) =>
      !device.transportIssue && latestTransportIssueByDevice[device.id]
        ? {
            ...device,
            transportIssue: latestTransportIssueByDevice[device.id],
          }
        : device
    )
    const selectedDeviceId = selectPreferredLiveDevdDeviceId(liveDevices)
    const fixtureDevices = scenario.devices.filter(
      (device) =>
        device.transport === 'mock' &&
        device.severity !== 'nominal' &&
        !liveDevices.some((liveDevice) => liveDevice.id === device.id)
    )
    const nativeTargets = liveDevices.filter((device) => device.transport === 'devd')
    const nativeCount = nativeTargets.length
    const activeNativeCount = nativeTargets.filter(isReadyLiveDevdTarget).length
    const devdEvent: EventLogEntry = {
      time: 'live',
      source: 'devd',
      message:
        activeNativeCount > 0
          ? `${activeNativeCount} authorized native target${activeNativeCount === 1 ? '' : 's'} discovered`
          : nativeCount > 0
            ? 'devd reachable; reconnecting the last authorized native serial target'
            : 'devd reachable; no authorized native serial target',
      tone: activeNativeCount > 0 ? 'success' : 'warning',
    }

    return {
      ...scenario,
      name: 'Live devd bridge',
      selectedDeviceId,
      devices: [...liveDevices, ...fixtureDevices],
      artifacts: artifacts.length > 0 ? artifacts : scenario.artifacts,
      metrics: scenario.metrics.map((metric) =>
        metric.label === 'Bound targets'
          ? {
              ...metric,
              value: String(devices.length + fixtureDevices.length).padStart(2, '0'),
              detail:
                activeNativeCount > 0
                  ? `${activeNativeCount} native, ${fixtureDevices.length} fixture`
                  : nativeCount > 0
                    ? `${nativeCount} native reconnecting, ${fixtureDevices.length} fixture`
                    : `${devices.length} daemon fixture, no native serial`,
              tone: activeNativeCount > 0 ? ('success' as const) : ('warning' as const),
            }
          : metric
      ),
      events: [devdEvent, ...devdEvents, ...scenario.events],
    }
  }, [artifacts, devdEvents, devices, latestTransportIssueByDevice, refreshState, scenario])
}

export function prioritizeLiveDevdDevices(devices: DeviceTarget[]) {
  return [...devices].sort((left, right) => liveDevicePriority(left) - liveDevicePriority(right))
}

export function degradeDevicesForRefreshError(devices: DeviceTarget[], error: unknown) {
  if (devices.length === 0) {
    return [createUnavailableLiveDevdTarget(issueMessage(error))]
  }

  const message = issueMessage(error)
  return devices.map<DeviceTarget>((device) => {
    if (device.transport !== 'devd') {
      return device
    }

    return {
      ...device,
      severity: 'warning' as const,
      networkState: 'error' as const,
      transportIssue: message,
    }
  })
}

export function preserveLastLiveDevdTarget(
  baseDevices: DeviceTarget[],
  currentDevices: DeviceTarget[],
  issue = DEVD_MISSING_LIVE_TARGET_MESSAGE
) {
  if (baseDevices.some((device) => device.transport === 'devd')) {
    return baseDevices
  }

  const lastLiveDevdTarget = prioritizeLiveDevdDevices(currentDevices).find(
    (device) => device.transport === 'devd'
  )
  if (!lastLiveDevdTarget) {
    return [createUnavailableLiveDevdTarget(issue), ...baseDevices]
  }

  return [
    {
      ...lastLiveDevdTarget,
      severity: 'warning' as const,
      networkState: 'error' as const,
      transportIssue: issue,
    },
    ...baseDevices.filter((device) => device.id !== lastLiveDevdTarget.id),
  ]
}

export function selectPreferredLiveDevdDeviceId(devices: DeviceTarget[]) {
  return devices.find((device) => device.transport === 'devd')?.id ?? devices[0]?.id ?? ''
}

export function mergeDevdProbeRecord(
  record: DevdDeviceRecord,
  live: {
    identity: Identity
    network: NetworkSummary
    status: ControlPlaneStatus
  }
): DevdDeviceRecord {
  return {
    ...record,
    connection: 'connected',
    identity: {
      ...live.identity,
      capabilities: mergeCapabilities(record.identity.capabilities, live.identity.capabilities),
    },
    network: live.network,
    status: live.status,
  }
}

function selectLiveDevdRecord(records: DevdDeviceRecord[]) {
  return (
    records.find(
      (record) => record.transport === 'native_serial' && record.connection !== 'busy'
    ) ?? records.find((record) => record.transport === 'native_serial')
  )
}

function replaceDevice(devices: DeviceTarget[], nextDevice: DeviceTarget) {
  return devices.map((device) => (device.id === nextDevice.id ? nextDevice : device))
}

function liveDevicePriority(device: DeviceTarget) {
  if (device.transport === 'devd' && device.leaseState === 'active') {
    return 0
  }
  if (device.transport === 'devd') {
    return 1
  }
  return 2
}

function isReadyLiveDevdTarget(device: DeviceTarget) {
  return (
    device.transport === 'devd' &&
    device.leaseState === 'active' &&
    device.networkState !== 'error' &&
    device.networkState !== 'timeout'
  )
}

export function createBootstrappingLiveDevdScenario(
  scenario: ControlPlaneScenario
): ControlPlaneScenario {
  const bootstrappingDevice = devdRecordToReconnectingTarget({
    id: 'live-devd-bootstrapping',
    displayName: 'Reconnecting devd target',
    portPath: 'Waiting for authorized native serial probe',
    transport: 'native_serial',
    connection: 'busy',
    identity: {
      deviceId: 'live-devd-bootstrapping',
      firmwareVersion: 'unknown',
      buildId: 'unknown',
      gitSha: 'unknown',
      board: 'esp32-s3',
      apiVersion: 'unknown',
      protocolVersion: 'flux-purr.usb.v1',
      hostname: 'live-devd-bootstrapping',
      capabilities: ['identity', 'status', 'monitor'],
    },
    network: {
      state: 'connecting',
      ssid: null,
      ip: null,
      gateway: null,
      dns: [],
      wifiRssi: null,
      lastError: null,
    },
    status: {
      uptimeSeconds: 0,
      currentTempC: 0,
      targetTempC: 30,
      boardTempCenti: 0,
      voltageMv: 0,
      currentMa: 0,
      pdRequestMv: 0,
      pdContractMv: 0,
      pdState: 'fault',
      manualPpsEnabled: false,
      manualPpsMv: null,
      manualPpsMa: null,
      ppsCapabilityMinMv: null,
      ppsCapabilityMaxMv: null,
      ppsCapabilityMaxMa: null,
      manualPpsError: null,
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
      heaterEnabled: false,
      heaterOutputPercent: 0,
      activeCoolingEnabled: false,
      fanDisplayState: 'OFF',
      selectedPresetSlot: 0,
      presetsC: [],
      heaterLockReason: null,
      frontpanelKey: null,
      mode: 'idle',
      fanEnabled: false,
      fanPwmPermille: 0,
      network: {
        state: 'connecting',
        ssid: null,
        ip: null,
        gateway: null,
        dns: [],
        wifiRssi: null,
        lastError: null,
      },
    },
    events: [],
  })

  const devdEvent: EventLogEntry = {
    time: 'live',
    source: 'devd',
    message: 'devd reachable; waiting for the first authorized native serial probe',
    tone: 'warning',
  }

  return {
    ...scenario,
    name: 'Live devd bridge',
    selectedDeviceId: bootstrappingDevice.id,
    devices: [bootstrappingDevice],
    metrics: scenario.metrics.map((metric) =>
      metric.label === 'Bound targets'
        ? {
            ...metric,
            value: '01',
            detail: 'native target reconnecting',
            tone: 'warning' as const,
          }
        : metric
    ),
    events: [devdEvent, ...scenario.events],
  }
}

function createUnavailableLiveDevdTarget(issue: string): DeviceTarget {
  return {
    id: DEVD_UNAVAILABLE_TARGET_ID,
    alias: 'Native devd target unavailable',
    location: 'Waiting for authorized native serial probe',
    transport: 'devd',
    severity: 'warning',
    baseUrl: 'devd://unavailable',
    firmware: 'unknown',
    buildId: 'unknown',
    uptime: 'unavailable',
    boardTempC: 0,
    currentTempC: 0,
    targetTempC: 30,
    voltageMv: 0,
    currentMa: 0,
    pdRequestMv: 0,
    pdContractMv: 0,
    pdState: 'fault',
    manualPpsEnabled: false,
    manualPpsMv: null,
    manualPpsMa: null,
    ppsCapabilityMinMv: null,
    ppsCapabilityMaxMv: null,
    ppsCapabilityMaxMa: null,
    manualPpsError: null,
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
    heaterEnabled: false,
    heaterOutputPercent: 0,
    activeCoolingEnabled: false,
    fanState: 'OFF',
    wifiRssi: null,
    networkState: 'error',
    leaseState: 'none',
    transportIssue: issue,
    capabilities: ['identity', 'status', 'monitor'],
  }
}

export function devdRecordToReconnectingTarget(record: DevdDeviceRecord): DeviceTarget {
  const reconnecting = devdRecordToDeviceTarget(record)
  return {
    ...reconnecting,
    severity: 'warning',
    networkState: 'connecting',
    leaseState: 'expired',
    leaseId: undefined,
    transportIssue: DEVD_RECONNECTING_MESSAGE,
  }
}

function mergeCapabilities(...capabilitySets: string[][]) {
  return Array.from(new Set(capabilitySets.flat()))
}

function devdRecordsToEvents(records: DevdDeviceRecord[]) {
  return records.flatMap((record) => record.events ?? []).slice(-DEVD_TRACE_LIMIT)
}

function devdEventsToLogEntries(recordEvents: DevdEvent[], streamEvents: DevdEvent[]) {
  return mergeDevdEvents(recordEvents, streamEvents).map(devdEventToLogEntry).reverse()
}

function latestDevdTransportIssueByDevice(recordEvents: DevdEvent[], streamEvents: DevdEvent[]) {
  const issuesByDevice: Record<string, string> = {}
  for (const event of mergeDevdEvents(recordEvents, streamEvents).reverse()) {
    if (!event.deviceId || issuesByDevice[event.deviceId]) {
      continue
    }
    if (!isMeaningfulDevdTransportIssueEvent(event)) {
      continue
    }
    issuesByDevice[event.deviceId] = devdEventToLogEntry(event).message
  }
  return issuesByDevice
}

function mergeDevdEvents(recordEvents: DevdEvent[], streamEvents: DevdEvent[]) {
  const eventsById = new Map<string, DevdEvent>()
  for (const event of [...recordEvents, ...streamEvents]) {
    eventsById.set(event.id, event)
  }

  return Array.from(eventsById.values())
    .sort((left, right) => Number(left.timestamp) - Number(right.timestamp))
    .slice(-DEVD_TRACE_LIMIT)
}

function appendDevdEvent(events: DevdEvent[], event: DevdEvent) {
  return mergeDevdEvents(events, [event])
}

function parseDevdEvent(data: string): DevdEvent | null {
  try {
    const event = JSON.parse(data) as DevdEvent
    return typeof event.id === 'string' && typeof event.kind === 'string' ? event : null
  } catch {
    return null
  }
}

function leaseStateForError(
  error: unknown,
  activeLease: DevdLease | null
): NonNullable<DeviceTarget['leaseState']> {
  if (error instanceof ControlPlaneClientError) {
    if (error.code === 'lease_conflict') {
      return 'conflict'
    }
    if (error.code === 'lease_expired' || error.code === 'lease_required') {
      return 'expired'
    }
  }
  return activeLease ? 'active' : 'none'
}

export function devdLeaseStorageKey(devdBaseUrl: string, deviceId: string) {
  return `${DEVD_LEASE_STORAGE_PREFIX}${devdBaseUrl}::${deviceId}`
}

export function devdDeviceSnapshotStorageKey(devdBaseUrl: string) {
  return `${DEVD_DEVICE_SNAPSHOT_STORAGE_PREFIX}${devdBaseUrl}`
}

interface DevdLeaseStorage {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
  removeItem(key: string): void
}

export function getDevdLeaseStorage(): DevdLeaseStorage | null {
  if (typeof window === 'undefined') {
    return null
  }

  return window.sessionStorage
}

export function readStoredDevdLeaseId(
  devdBaseUrl: string,
  deviceId: string,
  storage: DevdLeaseStorage | null = getDevdLeaseStorage()
) {
  if (!storage) {
    return null
  }

  try {
    return storage.getItem(devdLeaseStorageKey(devdBaseUrl, deviceId))
  } catch {
    return null
  }
}

export function writeStoredDevdLeaseId(
  devdBaseUrl: string,
  deviceId: string,
  leaseId: string,
  storage: DevdLeaseStorage | null = getDevdLeaseStorage()
) {
  if (!storage) {
    return
  }

  try {
    storage.setItem(devdLeaseStorageKey(devdBaseUrl, deviceId), leaseId)
  } catch {
    // Ignore storage failures; the in-memory lease still keeps the page live.
  }
}

export function releaseDevdLeaseOnPageHide({
  devdBaseUrl,
  lease,
  deviceId,
  storage,
  fetcher = fetch,
}: {
  devdBaseUrl: string
  lease: DevdLease
  deviceId: string
  storage: DevdLeaseStorage | null
  fetcher?: typeof fetch
}) {
  // Keep the lease id in session storage so a same-tab reload can heartbeat it
  // instead of colliding with its own still-live lease during the 8s TTL window.
  writeStoredDevdLeaseId(devdBaseUrl, deviceId, lease.leaseId, storage)
  bestEffortReleaseDevdLease(devdBaseUrl, lease.leaseId, fetcher)
}

export function clearStoredDevdLeaseId(
  devdBaseUrl: string,
  deviceId: string,
  storage: DevdLeaseStorage | null = getDevdLeaseStorage()
) {
  if (!storage) {
    return
  }

  try {
    storage.removeItem(devdLeaseStorageKey(devdBaseUrl, deviceId))
  } catch {
    // Ignore storage failures.
  }
}

export function readStoredLiveDevdTarget(
  devdBaseUrl: string,
  storage: DevdLeaseStorage | null = getDevdLeaseStorage()
) {
  if (!storage) {
    return []
  }

  try {
    const raw = storage.getItem(devdDeviceSnapshotStorageKey(devdBaseUrl))
    if (!raw) {
      return []
    }
    const parsed = JSON.parse(raw) as {
      capturedAt?: number
      device?: DeviceTarget
    }
    if (
      typeof parsed.capturedAt !== 'number' ||
      Date.now() - parsed.capturedAt > DEVD_DEVICE_SNAPSHOT_TTL_MS ||
      !parsed.device ||
      parsed.device.transport !== 'devd'
    ) {
      storage.removeItem(devdDeviceSnapshotStorageKey(devdBaseUrl))
      return []
    }
    return [rehydrateStoredLiveDevdTarget(parsed.device)]
  } catch {
    return []
  }
}

export function writeStoredLiveDevdTarget(
  devdBaseUrl: string,
  device: DeviceTarget,
  storage: DevdLeaseStorage | null = getDevdLeaseStorage()
) {
  if (!storage || device.transport !== 'devd') {
    return
  }

  try {
    storage.setItem(
      devdDeviceSnapshotStorageKey(devdBaseUrl),
      JSON.stringify({
        capturedAt: Date.now(),
        device,
      })
    )
  } catch {
    // Ignore storage failures; the live in-memory target still keeps the page active.
  }
}

function rehydrateStoredLiveDevdTarget(device: DeviceTarget): DeviceTarget {
  return {
    ...device,
    severity: 'warning',
    networkState: 'connecting',
    leaseState: 'expired',
    leaseId: undefined,
    transportIssue: DEVD_RECONNECTING_MESSAGE,
  }
}

export async function resolveDevdLease({
  client,
  devdBaseUrl,
  deviceId,
  currentLease,
  currentLeaseDeviceId,
  storage,
  cancelled,
}: {
  client: ControlPlaneHttpClient
  devdBaseUrl: string
  deviceId: string
  currentLease: DevdLease | null
  currentLeaseDeviceId: string | null
  storage: DevdLeaseStorage | null
  cancelled: () => boolean
}) {
  if (currentLease && currentLeaseDeviceId === deviceId) {
    writeStoredDevdLeaseId(devdBaseUrl, deviceId, currentLease.leaseId, storage)
    return client.heartbeatDevdLease(devdBaseUrl, currentLease.leaseId)
  }

  if (currentLease && currentLeaseDeviceId && currentLeaseDeviceId !== deviceId) {
    await client.releaseDevdLease(devdBaseUrl, currentLease.leaseId).catch(() => undefined)
  }

  const storedLeaseId = readStoredDevdLeaseId(devdBaseUrl, deviceId, storage)
  if (storedLeaseId) {
    try {
      const lease = await client.heartbeatDevdLease(devdBaseUrl, storedLeaseId)
      writeStoredDevdLeaseId(devdBaseUrl, deviceId, lease.leaseId, storage)
      return lease
    } catch (error) {
      clearStoredDevdLeaseId(devdBaseUrl, deviceId, storage)
      if (!isLeaseInvalid(error)) {
        throw error
      }
    }
  }

  const lease = await client.createDevdLease(devdBaseUrl, deviceId)
  if (cancelled()) {
    await client.releaseDevdLease(devdBaseUrl, lease.leaseId).catch(() => undefined)
    throw new ControlPlaneClientError(
      'devd lease acquisition was cancelled.',
      'lease_cancelled',
      true
    )
  }

  writeStoredDevdLeaseId(devdBaseUrl, deviceId, lease.leaseId, storage)
  return lease
}

function isLeaseInvalid(error: unknown) {
  return (
    error instanceof ControlPlaneClientError &&
    (error.code === 'lease_expired' ||
      error.code === 'lease_required' ||
      error.code === 'lease_device_mismatch')
  )
}

function issueMessage(error: unknown) {
  if (error instanceof ControlPlaneClientError) {
    return error.message
  }
  if (error instanceof Error) {
    return error.message
  }
  return 'devd bridge is unavailable.'
}
