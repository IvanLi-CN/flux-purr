import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type {
  CalibrationConfigRequest,
  CalibrationJobRequest,
  CalibrationJobState,
  CalibrationState,
  DirectRuntimeConfigRequest,
  HeaterCurvePackage,
  HeaterCurveState,
} from './contracts'
import { rememberKnownWebSerialDevice } from './known-web-serial-devices'
import { ControlPlaneClientError } from './transport-client'
import type { ControlPlaneScenario, DeviceTarget, EventLogEntry } from './types'
import {
  type BrowserSerialPort,
  formatWebSerialEventTime,
  getBrowserSerial,
  isWebSerialSupported,
  normalizeBrowserSerialError,
  selectBrowserSerialPort,
  type WebSerialConnectionState,
  WebSerialControlPlaneClient,
  type WebSerialDiagnostic,
  webSerialProbeToDeviceTarget,
} from './web-serial'

const WEB_SERIAL_POLL_MS = 1_000
const WEB_SERIAL_CONNECT_TIMEOUT_MS = 60_000

export interface LiveWebSerialOptions {
  enabled?: boolean
  clientFactory?: () => WebSerialControlPlaneClient
  connectTimeoutMs?: number
  persistKnownDevices?: boolean
}

export interface LiveWebSerialControls {
  state: WebSerialConnectionState
  supported: boolean
  preauthorizedPortsReady: boolean
  error?: string
  deviceId?: string
  connect: (options?: {
    forcePortSelection?: boolean
    replaceExisting?: boolean
    preauthorizedOnly?: boolean
    signal?: AbortSignal
  }) => Promise<boolean>
  disconnect: () => Promise<void>
  configureRuntime: (request: DirectRuntimeConfigRequest) => Promise<boolean>
  getCalibration: () => Promise<CalibrationState>
  getCalibrationJob: () => Promise<CalibrationJobState>
  configureCalibration: (
    request: Omit<CalibrationConfigRequest, 'leaseId'>
  ) => Promise<CalibrationState>
  configureCalibrationJob: (
    request: Omit<CalibrationJobRequest, 'leaseId'>
  ) => Promise<CalibrationJobState>
  getHeaterCurve: () => Promise<HeaterCurveState>
  previewHeaterCurve: (heaterCurve: HeaterCurvePackage) => Promise<HeaterCurveState>
  clearHeaterCurvePreview: () => Promise<HeaterCurveState>
  saveHeaterCurve: () => Promise<HeaterCurveState>
}

export function useLiveWebSerialScenario(
  scenario: ControlPlaneScenario,
  {
    enabled = true,
    clientFactory,
    connectTimeoutMs = WEB_SERIAL_CONNECT_TIMEOUT_MS,
    persistKnownDevices = false,
  }: LiveWebSerialOptions = {}
): { scenario: ControlPlaneScenario; serial: LiveWebSerialControls } {
  const browserSerial = getBrowserSerial()
  const supported = enabled && isWebSerialSupported(browserSerial)
  const clientRef = useRef<WebSerialControlPlaneClient | null>(null)
  const connectAttemptRef = useRef(0)
  const preauthorizedPortsRef = useRef<BrowserSerialPort[] | undefined>(undefined)
  const [preauthorizedPortsReady, setPreauthorizedPortsReady] = useState(
    !enabled || !browserSerial?.getPorts
  )
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

  useEffect(() => {
    preauthorizedPortsRef.current = undefined
    if (!enabled || !browserSerial?.getPorts) {
      setPreauthorizedPortsReady(true)
      return
    }

    setPreauthorizedPortsReady(false)
    let cancelled = false
    void browserSerial
      .getPorts()
      .then((ports) => {
        if (!cancelled) {
          preauthorizedPortsRef.current = ports
          setPreauthorizedPortsReady(true)
        }
      })
      .catch(() => {
        if (!cancelled) {
          preauthorizedPortsRef.current = undefined
          setPreauthorizedPortsReady(true)
        }
      })
    return () => {
      cancelled = true
    }
  }, [browserSerial, enabled])

  const appendEvent = useCallback((message: string, tone: EventLogEntry['tone'] = 'info') => {
    setEvents((current) =>
      [
        {
          time: formatWebSerialEventTime(new Date()),
          source: 'webserial',
          message,
          tone,
        },
        ...current,
      ].slice(0, 1_000)
    )
  }, [])

  const connect = useCallback(
    async ({
      forcePortSelection = false,
      replaceExisting = false,
      preauthorizedOnly = false,
      signal,
    }: {
      forcePortSelection?: boolean
      replaceExisting?: boolean
      preauthorizedOnly?: boolean
      signal?: AbortSignal
    } = {}) => {
      if (signal?.aborted) return false
      const attemptId = ++connectAttemptRef.current
      const isCurrentAttempt = () => connectAttemptRef.current === attemptId
      if (!enabled || !supported) {
        setError('Web Serial is not available in this browser.')
        setState('unsupported')
        return false
      }

      setState('connecting')
      setError(undefined)
      let client: WebSerialControlPlaneClient | null = null
      let connectionEstablished = false
      const replacingConnectedClient = replaceExisting && clientRef.current != null
      let previousClientDisconnected = false
      try {
        if (preauthorizedOnly && preauthorizedPortsRef.current?.length !== 1) {
          setError('没有唯一的已授权 Web Serial 端口，请手动选择设备。')
          setState('idle')
          return false
        }
        const selectedPortPromise =
          replaceExisting && !clientFactory && browserSerial
            ? selectBrowserSerialPort(
                browserSerial,
                preauthorizedPortsRef.current,
                forcePortSelection,
                !preauthorizedOnly
              )
            : null
        const selectedPort = selectedPortPromise ? await selectedPortPromise : null
        if (signal?.aborted || !isCurrentAttempt()) return false
        if (replaceExisting) {
          const currentClient = clientRef.current
          clientRef.current = null
          await currentClient?.disconnect()
          previousClientDisconnected = true
        }
        if (signal?.aborted || !isCurrentAttempt()) return false
        client =
          clientFactory?.() ??
          new WebSerialControlPlaneClient({
            serial: browserSerial,
            preauthorizedPorts: selectedPort ? [selectedPort] : preauthorizedPortsRef.current,
            requestPortWhenUnavailable: !preauthorizedOnly,
            onDiagnostic: (diagnostic) => {
              if (signal?.aborted || !isCurrentAttempt()) return
              const message = webSerialDiagnosticMessage(diagnostic)
              appendEvent(message, 'warning')
              if (connectionEstablished) {
                setDevice(null)
                setError(message)
                setState('error')
              }
            },
          })
        const probe = await withTimeout(
          client.connect(),
          connectTimeoutMs,
          new ControlPlaneClientError(
            'Web Serial 连接超时，请重新选择设备。',
            'web_serial_timeout',
            true
          )
        )
        if (signal?.aborted || !isCurrentAttempt()) {
          await client.disconnect()
          if (isCurrentAttempt()) setState('idle')
          return false
        }
        clientRef.current = client
        connectionEstablished = true
        const nextDevice = webSerialProbeToDeviceTarget(probe)
        if (persistKnownDevices) {
          rememberKnownWebSerialDevice({
            deviceId: probe.identity.deviceId,
            hostname: nextDevice.alias,
            firmwareVersion: probe.identity.firmwareVersion,
            buildId: probe.identity.buildId,
          })
        }
        setDevice(nextDevice)
        setState('connected')
        appendEvent(
          `${nextDevice.alias} USB JSONL probe accepted: get_identity / get_network / get_status`,
          'success'
        )
        appendEvent(`${nextDevice.alias} connected over browser Web Serial`, 'success')
        return true
      } catch (error) {
        if (signal?.aborted || !isCurrentAttempt()) {
          await client?.disconnect()
          if (isCurrentAttempt()) setState('idle')
          return false
        }
        const normalizedError = normalizeBrowserSerialError(error)
        const message = normalizedError.message
        if (replacingConnectedClient && !previousClientDisconnected) {
          setError(message)
          setState('connected')
          appendEvent('browser Web Serial device selection cancelled', 'warning')
          return false
        }
        setDevice(null)
        setError(message)
        setState('error')
        appendEvent('browser Web Serial connection failed', 'warning')
        await client?.disconnect()
        clientRef.current = null
        return false
      }
    },
    [
      appendEvent,
      browserSerial,
      clientFactory,
      connectTimeoutMs,
      enabled,
      persistKnownDevices,
      supported,
    ]
  )

  const disconnect = useCallback(async () => {
    connectAttemptRef.current += 1
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
                  apiVersion: '2026-05-29',
                  protocolVersion: 'flux-purr.usb.v1',
                  hostname: current.alias,
                  capabilities: current.capabilities,
                },
                network: status.network,
                status,
              })
            : current
        )
        appendEvent(
          [
            'runtime_config accepted over browser Web Serial',
            `target ${status.targetTempC}C`,
            `preset M${(status.selectedPresetSlot ?? 0) + 1}`,
            `cooling ${status.activeCoolingEnabled ? 'on' : 'off'}`,
            `heater ${status.heaterEnabled ? 'on' : 'off'}`,
            `fan ${status.fanDisplayState}`,
          ].join(' / '),
          'success'
        )
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

  const requireClient = useCallback(() => {
    const client = clientRef.current
    if (!client) {
      const message = 'Web Serial port is not connected.'
      setError(message)
      throw new Error(message)
    }
    return client
  }, [])

  const getCalibration = useCallback(async () => {
    try {
      const calibration = await requireClient().getCalibration()
      appendEvent('adc calibration read over browser Web Serial', 'success')
      return calibration
    } catch (error) {
      setError(error instanceof Error ? error.message : 'Web Serial calibration read failed.')
      setState('error')
      appendEvent('browser Web Serial calibration read failed', 'warning')
      throw error
    }
  }, [appendEvent, requireClient])

  const configureCalibration = useCallback(
    async (request: Omit<CalibrationConfigRequest, 'leaseId'>) => {
      try {
        const calibration = await requireClient().configureCalibration(request)
        appendEvent('adc calibration state updated over browser Web Serial', 'success')
        return calibration
      } catch (error) {
        setError(error instanceof Error ? error.message : 'Web Serial calibration update failed.')
        setState('error')
        appendEvent('browser Web Serial calibration update failed', 'warning')
        throw error
      }
    },
    [appendEvent, requireClient]
  )

  const getHeaterCurve = useCallback(async () => {
    try {
      const heaterCurve = await requireClient().getHeaterCurve()
      appendEvent('heater curve read over browser Web Serial', 'success')
      return heaterCurve
    } catch (error) {
      setError(error instanceof Error ? error.message : 'Web Serial heater curve read failed.')
      setState('error')
      appendEvent('browser Web Serial heater curve read failed', 'warning')
      throw error
    }
  }, [appendEvent, requireClient])

  const getCalibrationJob = useCallback(async () => {
    try {
      const job = await requireClient().getCalibrationJob()
      appendEvent('calibration auto job read over browser Web Serial', 'success')
      return job
    } catch (error) {
      setError(error instanceof Error ? error.message : 'Web Serial calibration job read failed.')
      setState('error')
      appendEvent('browser Web Serial calibration job read failed', 'warning')
      throw error
    }
  }, [appendEvent, requireClient])

  const configureCalibrationJob = useCallback(
    async (request: Omit<CalibrationJobRequest, 'leaseId'>) => {
      try {
        const job = await requireClient().configureCalibrationJob(request)
        appendEvent('calibration auto job command accepted over browser Web Serial', 'success')
        return job
      } catch (error) {
        setError(
          error instanceof Error ? error.message : 'Web Serial calibration job update failed.'
        )
        setState('error')
        appendEvent('browser Web Serial calibration job update failed', 'warning')
        throw error
      }
    },
    [appendEvent, requireClient]
  )

  const previewHeaterCurve = useCallback(
    async (heaterCurve: HeaterCurvePackage) => {
      try {
        const next = await requireClient().previewHeaterCurve(heaterCurve)
        appendEvent('heater curve preview accepted over browser Web Serial', 'success')
        return next
      } catch (error) {
        setError(error instanceof Error ? error.message : 'Web Serial heater curve preview failed.')
        setState('error')
        appendEvent('browser Web Serial heater curve preview failed', 'warning')
        throw error
      }
    },
    [appendEvent, requireClient]
  )

  const clearHeaterCurvePreview = useCallback(async () => {
    try {
      const next = await requireClient().clearHeaterCurvePreview()
      appendEvent('heater curve preview cleared over browser Web Serial', 'info')
      return next
    } catch (error) {
      setError(error instanceof Error ? error.message : 'Web Serial heater curve clear failed.')
      setState('error')
      appendEvent('browser Web Serial heater curve clear failed', 'warning')
      throw error
    }
  }, [appendEvent, requireClient])

  const saveHeaterCurve = useCallback(async () => {
    try {
      const next = await requireClient().saveHeaterCurve()
      appendEvent('heater curve saved over browser Web Serial', 'success')
      return next
    } catch (error) {
      setError(error instanceof Error ? error.message : 'Web Serial heater curve save failed.')
      setState('error')
      appendEvent('browser Web Serial heater curve save failed', 'warning')
      throw error
    }
  }, [appendEvent, requireClient])

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

  const serial = useMemo(
    () => ({
      state,
      supported,
      preauthorizedPortsReady,
      error,
      deviceId: device?.id,
      connect,
      disconnect,
      configureRuntime,
      getCalibration,
      getCalibrationJob,
      configureCalibration,
      configureCalibrationJob,
      getHeaterCurve,
      previewHeaterCurve,
      clearHeaterCurvePreview,
      saveHeaterCurve,
    }),
    [
      clearHeaterCurvePreview,
      configureCalibration,
      configureCalibrationJob,
      configureRuntime,
      connect,
      device?.id,
      disconnect,
      error,
      getCalibration,
      getCalibrationJob,
      getHeaterCurve,
      preauthorizedPortsReady,
      previewHeaterCurve,
      saveHeaterCurve,
      state,
      supported,
    ]
  )

  return {
    scenario: serialScenario,
    serial,
  }
}

function webSerialDiagnosticMessage(diagnostic: WebSerialDiagnostic) {
  if (diagnostic.kind === 'panic') {
    return `固件故障后复位：${diagnostic.reason}`
  }
  const reason =
    diagnostic.reason === 'system_brownout'
      ? '电源欠压'
      : diagnostic.reason === 'watchdog'
        ? '看门狗超时'
        : diagnostic.reason === 'software'
          ? '软件请求'
          : diagnostic.reason
  return `设备已复位：${reason}`
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, error: Error): Promise<T> {
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) {
    return promise
  }

  return new Promise<T>((resolve, reject) => {
    const timer = globalThis.setTimeout(() => reject(error), timeoutMs)
    promise.then(
      (value) => {
        globalThis.clearTimeout(timer)
        resolve(value)
      },
      (reason) => {
        globalThis.clearTimeout(timer)
        reject(reason)
      }
    )
  })
}
