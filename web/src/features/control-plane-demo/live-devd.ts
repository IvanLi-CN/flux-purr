import { useEffect, useMemo, useState } from 'react'
import type { DevdDeviceRecord, DevdLease } from './contracts'
import type { ControlPlaneHttpClient } from './transport-client'
import {
  ControlPlaneClientError,
  createControlPlaneHttpClient,
  devdRecordToDeviceTarget,
} from './transport-client'
import type { ControlPlaneScenario, DeviceTarget, EventLogEntry } from './types'

const DEVD_POLL_MS = 5_000

export interface LiveDevdOptions {
  enabled?: boolean
  devdBaseUrl?: string | null
  httpClient?: ControlPlaneHttpClient
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
  { enabled = true, devdBaseUrl = defaultDevdBaseUrl(), httpClient }: LiveDevdOptions = {}
) {
  const client = useMemo(() => httpClient ?? createControlPlaneHttpClient(), [httpClient])
  const [devices, setDevices] = useState<DeviceTarget[]>([])
  const [artifacts, setArtifacts] = useState<ControlPlaneScenario['artifacts']>([])

  useEffect(() => {
    if (!enabled || !devdBaseUrl) {
      setDevices([])
      setArtifacts([])
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
        }
        const baseDevices = records.map(devdRecordToDeviceTarget)
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
          const liveDevice = devdRecordToDeviceTarget({
            ...liveRecord,
            connection: 'connected',
            identity: live.identity,
            network: live.network,
            status: live.status,
          })
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
          setDevices(records.map(devdRecordToDeviceTarget))
          setArtifacts([])
        }
      }
    }

    void refresh()
    const timer = window.setInterval(refresh, DEVD_POLL_MS)

    return () => {
      cancelled = true
      window.clearInterval(timer)
      if (activeLease) {
        void client.releaseDevdLease(devdBaseUrl, activeLease.leaseId)
      }
    }
  }, [client, devdBaseUrl, enabled])

  return useMemo(() => {
    if (devices.length === 0) {
      return scenario
    }

    const fixtureDevices = scenario.devices.filter(
      (device) =>
        device.transport === 'mock' &&
        device.severity !== 'nominal' &&
        !devices.some((liveDevice) => liveDevice.id === device.id)
    )
    const nativeCount = devices.filter((device) => device.transport === 'devd').length
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
      selectedDeviceId: devices[0].id,
      devices: [...devices, ...fixtureDevices],
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
      events: [devdEvent, ...scenario.events],
    }
  }, [artifacts, devices, scenario])
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
