import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { DirectRuntimeConfigRequest } from './contracts'
import type { ControlPlaneScenario, DeviceTarget, EventLogEntry } from './types'
import {
  getBrowserSerial,
  isWebSerialSupported,
  type WebSerialConnectionState,
  WebSerialControlPlaneClient,
  webSerialProbeToDeviceTarget,
} from './web-serial'

const WEB_SERIAL_POLL_MS = 3_000

export interface LiveWebSerialOptions {
  enabled?: boolean
  clientFactory?: () => WebSerialControlPlaneClient
}

export interface LiveWebSerialControls {
  state: WebSerialConnectionState
  supported: boolean
  error?: string
  deviceId?: string
  connect: () => Promise<boolean>
  disconnect: () => Promise<void>
  configureRuntime: (request: DirectRuntimeConfigRequest) => Promise<boolean>
}

export function useLiveWebSerialScenario(
  scenario: ControlPlaneScenario,
  { enabled = true, clientFactory }: LiveWebSerialOptions = {}
): { scenario: ControlPlaneScenario; serial: LiveWebSerialControls } {
  const supported = enabled && isWebSerialSupported(getBrowserSerial())
  const clientRef = useRef<WebSerialControlPlaneClient | null>(null)
  const [state, setState] = useState<WebSerialConnectionState>(supported ? 'idle' : 'unsupported')
  const [device, setDevice] = useState<DeviceTarget | null>(null)
  const [events, setEvents] = useState<EventLogEntry[]>([])
  const [error, setError] = useState<string | undefined>()

  useEffect(() => {
    if (!enabled) {
      setState('unsupported')
      setDevice(null)
      setError(undefined)
      return
    }

    setState((current) => {
      if (!supported) {
        return 'unsupported'
      }
      return current === 'unsupported' ? 'idle' : current
    })
  }, [enabled, supported])

  const appendEvent = useCallback((message: string, tone: EventLogEntry['tone'] = 'info') => {
    setEvents((current) =>
      [
        {
          time: 'web',
          source: 'webserial',
          message,
          tone,
        },
        ...current,
      ].slice(0, 24)
    )
  }, [])

  const connect = useCallback(async () => {
    if (!enabled || !supported) {
      setError('Web Serial is not available in this browser.')
      setState('unsupported')
      return false
    }

    setState('connecting')
    setError(undefined)
    let client: WebSerialControlPlaneClient | null = null
    try {
      client = clientFactory?.() ?? new WebSerialControlPlaneClient()
      const probe = await client.connect()
      clientRef.current = client
      const nextDevice = webSerialProbeToDeviceTarget(probe)
      setDevice(nextDevice)
      setState('connected')
      appendEvent(`${nextDevice.alias} connected over browser Web Serial`, 'success')
      return true
    } catch (error) {
      await client?.disconnect()
      clientRef.current = null
      setDevice(null)
      setError(error instanceof Error ? error.message : 'Web Serial connection failed.')
      setState('error')
      appendEvent('browser Web Serial connection failed', 'warning')
      return false
    }
  }, [appendEvent, clientFactory, enabled, supported])

  const disconnect = useCallback(async () => {
    const client = clientRef.current
    clientRef.current = null
    setDevice(null)
    setState(supported ? 'idle' : 'unsupported')
    setError(undefined)
    appendEvent('browser Web Serial disconnected', 'info')
    await client?.disconnect()
  }, [appendEvent, supported])

  const refresh = useCallback(async () => {
    const client = clientRef.current
    if (!client) {
      return
    }

    try {
      const probe = await client.probe()
      setDevice(webSerialProbeToDeviceTarget(probe))
      setState('connected')
      setError(undefined)
    } catch (error) {
      setError(error instanceof Error ? error.message : 'Web Serial probe failed.')
      setState('error')
      appendEvent('browser Web Serial probe failed', 'warning')
    }
  }, [appendEvent])

  const configureRuntime = useCallback(
    async (request: DirectRuntimeConfigRequest) => {
      const client = clientRef.current
      if (!client) {
        setError('Web Serial port is not connected.')
        return false
      }

      try {
        const status = await client.configureRuntime(request)
        setDevice((current) =>
          current
            ? webSerialProbeToDeviceTarget({
                identity: {
                  deviceId: current.id.replace(/^web-serial-/, ''),
                  firmwareVersion: current.firmware,
                  buildId: current.buildId,
                  gitSha: 'unknown',
                  board: 'esp32-s3',
                  apiVersion: '2026-05-23',
                  protocolVersion: 'flux-purr.usb.v1',
                  hostname: current.alias,
                  capabilities: current.capabilities,
                },
                network: status.network,
                status,
              })
            : current
        )
        appendEvent('runtime config applied over browser Web Serial', 'success')
        return true
      } catch (error) {
        setError(error instanceof Error ? error.message : 'Web Serial runtime update failed.')
        setState('error')
        appendEvent('browser Web Serial runtime update failed', 'warning')
        return false
      }
    },
    [appendEvent]
  )

  useEffect(() => {
    if (state !== 'connected') {
      return
    }

    const timer = window.setInterval(refresh, WEB_SERIAL_POLL_MS)
    return () => window.clearInterval(timer)
  }, [refresh, state])

  useEffect(
    () => () => {
      void clientRef.current?.disconnect()
    },
    []
  )

  const serialScenario = useMemo(() => {
    if (!device) {
      return scenario
    }

    const devices = [device, ...scenario.devices.filter((item) => item.id !== device.id)]
    return {
      ...scenario,
      name: 'Browser Web Serial',
      selectedDeviceId: device.id,
      devices,
      metrics: scenario.metrics.map((metric) =>
        metric.label === 'Bound targets'
          ? {
              ...metric,
              value: String(devices.length).padStart(2, '0'),
              detail: 'browser serial + available targets',
              tone: 'success' as const,
            }
          : metric
      ),
      events: [...events, ...scenario.events],
    }
  }, [device, events, scenario])

  return {
    scenario: serialScenario,
    serial: {
      state,
      supported,
      error,
      deviceId: device?.id,
      connect,
      disconnect,
      configureRuntime,
    },
  }
}
