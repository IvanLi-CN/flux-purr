import { useEffect, useMemo, useState } from 'react'
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
  ControlPlaneClientError,
  createControlPlaneHttpClient,
  devdEventToLogEntry,
  devdRecordToDeviceTarget,
} from './transport-client'
import type { ControlPlaneScenario, DeviceTarget, EventLogEntry } from './types'

const DEVD_POLL_MS = 2_000
const DEVD_TRACE_LIMIT = 1_000
const DEVD_EVENT_KINDS = ['serial', 'lease', 'wifi', 'runtime', 'flash', 'transport'] as const

export interface LiveDevdOptions {
  enabled?: boolean
  devdBaseUrl?: string | null
  httpClient?: ControlPlaneHttpClient
  includeMockDevices?: boolean
}

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
  const [devices, setDevices] = useState<DeviceTarget[]>([])
  const [artifacts, setArtifacts] = useState<ControlPlaneScenario['artifacts']>([])
  const [recordEvents, setRecordEvents] = useState<DevdEvent[]>([])
  const [streamEvents, setStreamEvents] = useState<DevdEvent[]>([])
  const liveDevdDeviceId = useMemo(
    () => devices.find((device) => device.transport === 'devd')?.id,
    [devices]
  )

  useEffect(() => {
    if (!enabled || !devdBaseUrl) {
      setDevices([])
      setArtifacts([])
      setRecordEvents([])
      setStreamEvents([])
      return
    }

    let cancelled = false
    let activeLease: DevdLease | null = null

    const refresh = async () => {
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
          if (!cancelled) {
            setDevices(baseDevices)
          }
          return
        }

        try {
          if (!activeLease || activeLease.deviceId !== liveRecord.id) {
            if (activeLease) {
              void client.releaseDevdLease(devdBaseUrl, activeLease.leaseId)
            }
            activeLease = await client.createDevdLease(devdBaseUrl, liveRecord.id)
          } else {
            activeLease = await client.heartbeatDevdLease(devdBaseUrl, activeLease.leaseId)
          }

          const live = await client.probeDevdDevice(devdBaseUrl, liveRecord.id, activeLease.leaseId)
          const liveDevice = devdRecordToDeviceTarget(mergeDevdProbeRecord(liveRecord, live))
          liveDevice.leaseState = 'active'
          liveDevice.leaseId = activeLease.leaseId

          if (!cancelled) {
            setDevices(replaceDevice(baseDevices, liveDevice))
          }
        } catch (error) {
          const failedLease = activeLease
          if (isLeaseInvalid(error)) {
            activeLease = null
          }
          const issueDevice = devdRecordToDeviceTarget(liveRecord)
          issueDevice.severity = 'warning'
          issueDevice.leaseState = leaseStateForError(error, failedLease)
          issueDevice.leaseId = failedLease?.leaseId
          issueDevice.transportIssue = issueMessage(error)
          if (!cancelled) {
            setDevices(replaceDevice(baseDevices, issueDevice))
          }
        }
      } catch {
        if (!cancelled) {
          setDevices([])
          setArtifacts([])
          setRecordEvents([])
          setStreamEvents([])
        }
      }
    }

    void refresh()
    const timer = window.setInterval(refresh, DEVD_POLL_MS)

    return () => {
      cancelled = true
      window.clearInterval(timer)
      setStreamEvents([])
      if (activeLease) {
        void client.releaseDevdLease(devdBaseUrl, activeLease.leaseId)
      }
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

  return useMemo(() => {
    if (devices.length === 0) {
      return scenario
    }

    const liveDevices = prioritizeLiveDevdDevices(devices)
    const selectedDeviceId = selectPreferredLiveDevdDeviceId(liveDevices)
    const fixtureDevices = scenario.devices.filter(
      (device) =>
        device.transport === 'mock' &&
        device.severity !== 'nominal' &&
        !liveDevices.some((liveDevice) => liveDevice.id === device.id)
    )
    const nativeCount = liveDevices.filter((device) => device.transport === 'devd').length
    const devdEvent: EventLogEntry = {
      time: 'live',
      source: 'devd',
      message:
        nativeCount > 0
          ? `${nativeCount} authorized native target${nativeCount === 1 ? '' : 's'} discovered`
          : 'devd reachable; no authorized native serial target',
      tone: nativeCount > 0 ? 'success' : 'warning',
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
                nativeCount > 0
                  ? `${nativeCount} native, ${fixtureDevices.length} fixture`
                  : `${devices.length} daemon fixture, no native serial`,
              tone: nativeCount > 0 ? ('success' as const) : ('warning' as const),
            }
          : metric
      ),
      events: [devdEvent, ...devdEvents, ...scenario.events],
    }
  }, [artifacts, devdEvents, devices, scenario])
}

export function prioritizeLiveDevdDevices(devices: DeviceTarget[]) {
  return [...devices].sort((left, right) => liveDevicePriority(left) - liveDevicePriority(right))
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

function mergeCapabilities(...capabilitySets: string[][]) {
  return Array.from(new Set(capabilitySets.flat()))
}

function devdRecordsToEvents(records: DevdDeviceRecord[]) {
  return records.flatMap((record) => record.events ?? []).slice(-DEVD_TRACE_LIMIT)
}

function devdEventsToLogEntries(recordEvents: DevdEvent[], streamEvents: DevdEvent[]) {
  return mergeDevdEvents(recordEvents, streamEvents).map(devdEventToLogEntry).reverse()
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
