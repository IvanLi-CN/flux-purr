import { useEffect, useMemo, useState } from 'react'
import type { ControlPlaneHttpClient } from './transport-client'
import { createControlPlaneHttpClient, devdRecordToDeviceTarget } from './transport-client'
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

  useEffect(() => {
    if (!enabled || !devdBaseUrl) {
      setDevices([])
      return
    }

    let cancelled = false

    const refresh = async () => {
      try {
        const records = await client.listDevdDevices(devdBaseUrl)
        if (!cancelled) {
          setDevices(records.map(devdRecordToDeviceTarget))
        }
      } catch {
        if (!cancelled) {
          setDevices([])
        }
      }
    }

    void refresh()
    const timer = window.setInterval(refresh, DEVD_POLL_MS)

    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [client, devdBaseUrl, enabled])

  return useMemo(() => {
    if (devices.length === 0) {
      return scenario
    }

    const mockDevices = scenario.devices.filter(
      (device) => !devices.some((liveDevice) => liveDevice.id === device.id)
    )
    const devdEvent: EventLogEntry = {
      time: 'live',
      source: 'devd',
      message: `${devices.length} live target${devices.length === 1 ? '' : 's'} discovered`,
      tone: 'success',
    }

    return {
      ...scenario,
      name: 'Live devd bridge',
      selectedDeviceId: devices[0].id,
      devices: [...devices, ...mockDevices],
      metrics: scenario.metrics.map((metric) =>
        metric.label === 'Bound targets'
          ? {
              ...metric,
              value: String(devices.length + mockDevices.length).padStart(2, '0'),
              detail: `${devices.length} live devd, ${mockDevices.length} fixture`,
              tone: 'success' as const,
            }
          : metric
      ),
      events: [devdEvent, ...scenario.events],
    }
  }, [devices, scenario])
}
