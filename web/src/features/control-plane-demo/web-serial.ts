import type {
  ApiErrorEnvelope,
  CalibrationConfigRequest,
  CalibrationJobRequest,
  CalibrationJobState,
  CalibrationState,
  ControlPlaneStatus,
  DirectRuntimeConfigRequest,
  HeaterCurvePackage,
  HeaterCurveState,
  Identity,
  InstallStatus,
  NetworkSummary,
  UsbCalibrationConfigFrame,
  UsbCalibrationJobFrame,
  UsbHeaterCurveConfigFrame,
  UsbHeaterCurveSaveFrame,
  UsbRequestFrame,
  UsbRuntimeConfigFrame,
} from './contracts'
import { ControlPlaneClientError } from './transport-client'
import type { DeviceTarget } from './types'

const WEB_SERIAL_BAUD_RATE = 115_200
const WEB_SERIAL_RPC_TIMEOUT_MS = 12_000
const WEB_SERIAL_DEVICE_BASE_URL = 'webserial://selected'
const WEB_SERIAL_LINE_LIMIT = 8 * 1024
export const WEB_SERIAL_INITIALIZATION_TIMEOUT_MS = 8_000
const WEB_SERIAL_INITIAL_REQUEST_TIMEOUT_MS = 2_000
const WEB_SERIAL_CLOSE_TIMEOUT_MS = 4_000
const WEB_SERIAL_CLOSE_RETRY_MS = 100

export interface BrowserSerialPortFilter {
  readonly usbVendorId: number
  readonly usbProductId?: number
}

export interface BrowserSerialRequestOptions {
  readonly filters?: readonly BrowserSerialPortFilter[]
}

// ESP32-S3's native USB Serial/JTAG controller identifies as Espressif 303A:1001.
export const FLUX_PURR_USB_SERIAL_REQUEST_OPTIONS: BrowserSerialRequestOptions = {
  filters: [{ usbVendorId: 0x303a, usbProductId: 0x1001 }],
}

export type WebSerialConnectionState = 'unsupported' | 'idle' | 'connecting' | 'connected' | 'error'

export interface WebSerialDiagnostic {
  kind: 'boot_stage' | 'reset' | 'panic'
  reason: string
}

export interface WebSerialInitializationRetry {
  attempt: number
  remainingMs: number
}

export interface BrowserSerial {
  requestPort(options?: BrowserSerialRequestOptions): Promise<BrowserSerialPort>
  getPorts?(): Promise<BrowserSerialPort[]>
}

export interface BrowserSerialPort {
  readable: ReadableStream<Uint8Array> | null
  writable: WritableStream<Uint8Array> | null
  getInfo?(): { usbVendorId?: number; usbProductId?: number }
  open(options: { baudRate: number; bufferSize?: number }): Promise<void>
  close(): Promise<void>
}

export function isFluxPurrUsbSerialPort(port: BrowserSerialPort) {
  const info = port.getInfo?.()
  return info?.usbVendorId === 0x303a && info.usbProductId === 0x1001
}

/**
 * `navigator.serial.requestPort()` must begin during the triggering click. The
 * caller supplies any port list discovered before that click so this function
 * can either reuse one port or synchronously open the native chooser.
 */
export function selectBrowserSerialPort(
  serial: BrowserSerial,
  preauthorizedPorts?: readonly BrowserSerialPort[],
  forcePortSelection = false,
  requestPortWhenUnavailable = true
): Promise<BrowserSerialPort> {
  if (forcePortSelection) {
    return serial.requestPort(FLUX_PURR_USB_SERIAL_REQUEST_OPTIONS)
  }
  const preauthorizedPort = preauthorizedPorts?.length === 1 ? preauthorizedPorts[0] : null
  if (preauthorizedPort) return Promise.resolve(preauthorizedPort)
  if (requestPortWhenUnavailable) {
    return serial.requestPort(FLUX_PURR_USB_SERIAL_REQUEST_OPTIONS)
  }
  return Promise.reject(
    new ControlPlaneClientError(
      '没有唯一的已授权 Web Serial 端口，请手动选择设备。',
      'web_serial_port_required',
      true
    )
  )
}

export interface WebSerialProbe {
  identity: Identity
  network: NetworkSummary
  status: ControlPlaneStatus
}

interface UsbResponseWire {
  type?: string
  requestId?: string
  ok?: boolean
  result?: Record<string, unknown>
  error?: ApiErrorEnvelope['error']
}

interface PendingRequest {
  resolve: (payload: Record<string, unknown>) => void
  reject: (error: Error) => void
  timeout: ReturnType<typeof setTimeout>
}

interface RuntimeReadyWaiter {
  resolve: () => void
  reject: (error: Error) => void
  timeout?: ReturnType<typeof setTimeout>
}

type UsbFrameFactory = (
  requestId: string
) =>
  | UsbRequestFrame
  | UsbRuntimeConfigFrame
  | UsbCalibrationJobFrame
  | UsbCalibrationConfigFrame
  | UsbHeaterCurveConfigFrame
  | UsbHeaterCurveSaveFrame

export function getBrowserSerial(): BrowserSerial | null {
  if (typeof navigator === 'undefined') {
    return null
  }

  return ((navigator as Navigator & { serial?: BrowserSerial }).serial ??
    null) as BrowserSerial | null
}

export function isWebSerialSupported(serial: BrowserSerial | null = getBrowserSerial()) {
  return Boolean(serial)
}

export function isDirectWebSerialDevice(device: Pick<DeviceTarget, 'baseUrl' | 'transport'>) {
  return device.transport === 'serial' && device.baseUrl === WEB_SERIAL_DEVICE_BASE_URL
}

export function webSerialProbeToDeviceTarget(probe: WebSerialProbe): DeviceTarget {
  return {
    id: `web-serial-${probe.identity.deviceId}`,
    identityId: probe.identity.deviceId,
    alias: probe.identity.hostname || probe.identity.deviceId,
    location: 'Browser Web Serial',
    transport: 'serial',
    severity: 'nominal',
    baseUrl: WEB_SERIAL_DEVICE_BASE_URL,
    firmware: probe.identity.firmwareVersion,
    buildId: probe.identity.buildId,
    uptime: formatUptime(probe.status.uptimeSeconds),
    boardTempC: probe.status.boardTempCenti / 100,
    currentTempC: probe.status.currentTempC,
    targetTempC: probe.status.targetTempC,
    selectedPresetIndex: probe.status.selectedPresetSlot,
    presetsC: probe.status.presetsC,
    rtdRawAdcMv: probe.status.rtdRawAdcMv,
    vinRawAdcMv: probe.status.vinRawAdcMv,
    voltageMv: probe.status.voltageMv,
    currentMa: probe.status.currentMa,
    pdRequestMv: probe.status.pdRequestMv,
    pdContractMv: probe.status.pdContractMv,
    pdState: probe.status.pdState,
    pdController: probe.status.pdController ?? null,
    pdContractKind: probe.status.pdContractKind ?? null,
    pdContractCurrentMa: probe.status.pdContractCurrentMa ?? null,
    pdContractPowerMw: probe.status.pdContractPowerMw ?? null,
    pdPerformanceGuaranteed: probe.status.pdPerformanceGuaranteed ?? null,
    pdDegradedReason: probe.status.pdDegradedReason ?? null,
    manualPpsEnabled: probe.status.manualPpsEnabled ?? false,
    manualPpsMv: probe.status.manualPpsMv ?? null,
    manualPpsMa: probe.status.manualPpsMa ?? null,
    ppsCapabilityMinMv: probe.status.ppsCapabilityMinMv ?? null,
    ppsCapabilityMaxMv: probe.status.ppsCapabilityMaxMv ?? null,
    ppsCapabilityMaxMa: probe.status.ppsCapabilityMaxMa ?? null,
    manualPpsError: probe.status.manualPpsError ?? null,
    faultAttentionPending: probe.status.faultAttentionPending ?? false,
    calibration: probe.status.calibration,
    heaterEnabled: probe.status.heaterEnabled,
    heaterOutputPercent: probe.status.heaterOutputPercent,
    activeCoolingEnabled: probe.status.activeCoolingEnabled,
    fanState: probe.status.fanDisplayState,
    wifiSsid: probe.network.ssid ?? null,
    wifiRssi: probe.network.wifiRssi ?? null,
    wifiPasswordLength: probe.network.wifiPasswordLength ?? 0,
    capabilities: mergeCapabilities(probe.identity.capabilities, [
      'usb_jsonl',
      'status',
      'monitor',
    ]),
    networkState: probe.network.state,
    leaseState: 'active',
  }
}

export class WebSerialControlPlaneClient {
  private readonly serial: BrowserSerial
  private readonly baudRate: number
  private readonly encoder = new TextEncoder()
  private readonly decoder = new TextDecoder()
  private readonly pending = new Map<string, PendingRequest>()
  private readonly preauthorizedPorts?: readonly BrowserSerialPort[]
  private readonly onDiagnostic?: (diagnostic: WebSerialDiagnostic) => void
  private readonly onInitializationRetry?: (retry: WebSerialInitializationRetry) => void
  private readonly requestPortWhenUnavailable: boolean
  private port: BrowserSerialPort | null = null
  private reader: ReadableStreamDefaultReader<Uint8Array> | null = null
  private lineBuffer = ''
  private readPump: Promise<void> | null = null
  private writeChain = Promise.resolve()
  private connectionAttempt = 0
  private runtimeReadyObserved = false
  private readonly runtimeReadyWaiters = new Set<RuntimeReadyWaiter>()

  constructor({
    serial = getBrowserSerial(),
    baudRate = WEB_SERIAL_BAUD_RATE,
    preauthorizedPorts,
    onDiagnostic,
    onInitializationRetry,
    requestPortWhenUnavailable = true,
  }: {
    serial?: BrowserSerial | null
    baudRate?: number
    preauthorizedPorts?: readonly BrowserSerialPort[]
    onDiagnostic?: (diagnostic: WebSerialDiagnostic) => void
    onInitializationRetry?: (retry: WebSerialInitializationRetry) => void
    requestPortWhenUnavailable?: boolean
  } = {}) {
    if (!serial) {
      throw new ControlPlaneClientError(
        'Web Serial is not available in this browser.',
        'web_serial_unsupported',
        false
      )
    }
    this.serial = serial
    this.baudRate = baudRate
    this.preauthorizedPorts = preauthorizedPorts
    this.onDiagnostic = onDiagnostic
    this.onInitializationRetry = onInitializationRetry
    this.requestPortWhenUnavailable = requestPortWhenUnavailable
  }

  async connect() {
    const attempt = ++this.connectionAttempt
    this.runtimeReadyObserved = false
    this.lineBuffer = ''
    let port: BrowserSerialPort
    try {
      port = await selectBrowserSerialPort(
        this.serial,
        this.preauthorizedPorts,
        false,
        this.requestPortWhenUnavailable
      )
    } catch (error) {
      throw normalizeBrowserSerialError(error)
    }
    if (attempt !== this.connectionAttempt) {
      await port.close().catch(() => undefined)
      throw new ControlPlaneClientError('Web Serial connection closed.', 'web_serial_closed', true)
    }
    await port.open({ baudRate: this.baudRate })
    if (attempt !== this.connectionAttempt) {
      await port.close().catch(() => undefined)
      throw new ControlPlaneClientError('Web Serial connection closed.', 'web_serial_closed', true)
    }
    this.port = port
    this.readPump = this.readLoop()
    try {
      return await this.probeAfterInitialization()
    } catch (error) {
      await this.disconnect()
      throw error
    }
  }

  async disconnect() {
    this.connectionAttempt += 1
    const port = this.port
    this.port = null
    const closed = new ControlPlaneClientError(
      'Web Serial connection closed.',
      'web_serial_closed',
      true
    )
    this.rejectAll(closed)
    this.rejectRuntimeReadyWaiters(closed)
    await withCleanupTimeout(this.reader?.cancel().catch(() => undefined))
    await withCleanupTimeout(this.readPump?.catch(() => undefined))
    if (port) {
      await closeBrowserSerialPort(port)
    }
  }

  async probe(): Promise<WebSerialProbe> {
    return this.probeWithDeadline()
  }

  private async probeWithDeadline(deadline?: number): Promise<WebSerialProbe> {
    const timeout = () => {
      if (deadline === undefined) return WEB_SERIAL_RPC_TIMEOUT_MS
      const remaining = deadline - Date.now()
      if (remaining <= 0) throw runtimeInitializationTimeoutError()
      return Math.min(WEB_SERIAL_RPC_TIMEOUT_MS, remaining)
    }
    const identity = await this.requestPayload<Identity>(
      'identity',
      createUsbRequestFrame('get_identity'),
      timeout()
    )
    const network = await this.requestPayload<NetworkSummary>(
      'network',
      createUsbRequestFrame('get_network'),
      timeout()
    )
    const status = await this.requestPayload<ControlPlaneStatus>(
      'status',
      createUsbRequestFrame('get_status'),
      timeout()
    )
    return { identity, network, status }
  }

  get connectedPort(): BrowserSerialPort {
    return this.requireOpenPort()
  }

  async getInstallStatus(): Promise<InstallStatus> {
    return this.requestPayload<InstallStatus>(
      'install_status',
      createUsbRequestFrame('get_install_status')
    )
  }

  private async probeAfterInitialization(): Promise<WebSerialProbe> {
    const deadline = Date.now() + WEB_SERIAL_INITIALIZATION_TIMEOUT_MS
    const identity = await this.requestPayloadAfterRuntimeReady<Identity>(
      'identity',
      createUsbRequestFrame('get_identity'),
      deadline
    )
    const network = await this.requestPayloadAfterRuntimeReady<NetworkSummary>(
      'network',
      createUsbRequestFrame('get_network'),
      deadline
    )
    const status = await this.requestPayloadAfterRuntimeReady<ControlPlaneStatus>(
      'status',
      createUsbRequestFrame('get_status'),
      deadline
    )
    return { identity, network, status }
  }

  async configureRuntime(request: DirectRuntimeConfigRequest): Promise<ControlPlaneStatus> {
    return this.requestPayload<ControlPlaneStatus>('status', (requestId) => ({
      type: 'runtime_config',
      requestId,
      ...request,
    }))
  }

  async getCalibration(): Promise<CalibrationState> {
    return this.requestPayload<CalibrationState>(
      'calibration',
      createUsbRequestFrame('get_calibration')
    )
  }

  async configureCalibration(
    request: Omit<CalibrationConfigRequest, 'leaseId'>
  ): Promise<CalibrationState> {
    return this.requestPayload<CalibrationState>('calibration', (requestId) => ({
      type: 'calibration_config',
      requestId,
      ...request,
    }))
  }

  async getHeaterCurve(): Promise<HeaterCurveState> {
    return this.requestPayload<HeaterCurveState>(
      'heater_curve',
      createUsbRequestFrame('get_heater_curve')
    )
  }

  async previewHeaterCurve(heaterCurve: HeaterCurvePackage): Promise<HeaterCurveState> {
    return this.requestPayload<HeaterCurveState>('heater_curve', (requestId) => ({
      type: 'heater_curve_config',
      requestId,
      op: 'preview',
      heaterCurve,
    }))
  }

  async clearHeaterCurvePreview(): Promise<HeaterCurveState> {
    return this.requestPayload<HeaterCurveState>('heater_curve', (requestId) => ({
      type: 'heater_curve_config',
      requestId,
      op: 'clear_preview',
    }))
  }

  async saveHeaterCurve(): Promise<HeaterCurveState> {
    return this.requestPayload<HeaterCurveState>('heater_curve', (requestId) => ({
      type: 'heater_curve_save',
      requestId,
    }))
  }

  async getCalibrationJob(): Promise<CalibrationJobState> {
    return this.requestPayload<CalibrationJobState>(
      'calibration_job',
      createUsbRequestFrame('get_calibration_job')
    )
  }

  async configureCalibrationJob(
    request: Omit<CalibrationJobRequest, 'leaseId'>
  ): Promise<CalibrationJobState> {
    return this.requestPayload<CalibrationJobState>('calibration_job', (requestId) => ({
      type: 'calibration_job',
      requestId,
      ...request,
    }))
  }

  private async requestPayload<T>(
    payloadKey: string,
    frameFactory: UsbFrameFactory,
    timeoutMs = WEB_SERIAL_RPC_TIMEOUT_MS
  ): Promise<T> {
    const requestId = createWebSerialRequestId()
    const result = await this.exchange(frameFactory(requestId), timeoutMs)
    const payload = result[payloadKey]

    if (!payload || typeof payload !== 'object') {
      throw new ControlPlaneClientError(
        `USB response did not include ${payloadKey}.`,
        'usb_payload_missing',
        true
      )
    }

    return payload as T
  }

  /**
   * Opening ESP32-S3 USB Serial/JTAG can reset the target. The first request
   * is allowed to observe that startup state, but it is never retried on a
   * timer: firmware's `boot_stage=runtime_ready` is the sole retry boundary.
   */
  private async requestPayloadAfterRuntimeReady<T>(
    payloadKey: string,
    frameFactory: UsbFrameFactory,
    deadline: number
  ): Promise<T> {
    const remaining = deadline - Date.now()
    if (remaining <= 0) throw runtimeInitializationTimeoutError()

    try {
      return await this.requestPayload<T>(
        payloadKey,
        frameFactory,
        Math.min(WEB_SERIAL_INITIAL_REQUEST_TIMEOUT_MS, remaining)
      )
    } catch (error) {
      if (!isFirmwareInitializationPending(error)) throw error
    }

    const retryRemaining = deadline - Date.now()
    if (retryRemaining <= 0) throw runtimeInitializationTimeoutError()
    this.onInitializationRetry?.({ attempt: 1, remainingMs: retryRemaining })
    await this.waitForRuntimeReady(deadline)

    const finalRemaining = deadline - Date.now()
    if (finalRemaining <= 0) throw runtimeInitializationTimeoutError()
    return this.requestPayload<T>(payloadKey, frameFactory, finalRemaining)
  }

  private exchange(frame: ReturnType<UsbFrameFactory>, timeoutMs = WEB_SERIAL_RPC_TIMEOUT_MS) {
    const port = this.requireOpenPort()
    if (!port.writable) {
      throw new ControlPlaneClientError(
        'Web Serial port is not writable.',
        'web_serial_not_writable',
        true
      )
    }

    const requestId = frame.requestId
    const payload = `${JSON.stringify(frame)}\n`

    const response = new Promise<Record<string, unknown>>((resolve, reject) => {
      const timeout = globalThis.setTimeout(() => {
        this.pending.delete(requestId)
        reject(
          new ControlPlaneClientError(
            'Timed out waiting for a matching USB JSONL response.',
            'usb_response_timeout',
            true
          )
        )
      }, timeoutMs)
      this.pending.set(requestId, { resolve, reject, timeout })
    })

    const write = this.writeChain
      .catch(() => undefined)
      .then(async () => {
        const writer = port.writable?.getWriter()
        if (!writer) {
          throw new ControlPlaneClientError(
            'Web Serial port is not writable.',
            'web_serial_not_writable',
            true
          )
        }
        try {
          await writer.write(this.encoder.encode(payload))
        } finally {
          writer.releaseLock()
        }
      })
    this.writeChain = write

    return write
      .then(() => response)
      .catch((error) => {
        const pending = this.pending.get(requestId)
        const wrappedError = normalizeBrowserSerialError(error)
        if (pending) {
          globalThis.clearTimeout(pending.timeout)
          this.pending.delete(requestId)
          pending.reject(wrappedError)
        }
        throw wrappedError
      })
  }

  private requireOpenPort() {
    if (!this.port) {
      throw new ControlPlaneClientError(
        'Web Serial port is not connected.',
        'web_serial_not_connected',
        true
      )
    }
    return this.port
  }

  private async readLoop() {
    const port = this.requireOpenPort()
    if (!port.readable) {
      this.rejectAll(
        new ControlPlaneClientError(
          'Web Serial port is not readable.',
          'web_serial_not_readable',
          true
        )
      )
      return
    }

    const reader = port.readable.getReader()
    this.reader = reader
    try {
      while (this.port === port) {
        const { value, done } = await reader.read()
        if (done) {
          this.rejectAll(
            new ControlPlaneClientError(
              'Web Serial stream closed before a USB JSONL response.',
              'web_serial_stream_closed',
              true
            )
          )
          break
        }
        if (value) {
          this.consumeSerialText(this.decoder.decode(value, { stream: true }))
        }
      }
    } catch (error) {
      this.rejectAll(normalizeBrowserSerialError(error))
    } finally {
      if (this.reader === reader) {
        this.reader = null
      }
      reader.releaseLock()
    }
  }

  private consumeSerialText(text: string) {
    this.lineBuffer += text
    if (this.lineBuffer.length > WEB_SERIAL_LINE_LIMIT) {
      this.lineBuffer = ''
    }

    let newlineIndex = this.lineBuffer.indexOf('\n')
    while (newlineIndex >= 0) {
      const line = this.lineBuffer.slice(0, newlineIndex).trim()
      this.lineBuffer = this.lineBuffer.slice(newlineIndex + 1)
      this.decodeResponseLine(line)
      newlineIndex = this.lineBuffer.indexOf('\n')
    }
  }

  private decodeResponseLine(line: string) {
    if (!line) {
      return
    }

    const frame = parseUsbResponseWire(line)
    if (!frame) {
      const diagnostic = parseWebSerialDiagnostic(line)
      if (diagnostic) {
        this.observeDiagnostic(diagnostic)
      }
      return
    }

    if (frame.type !== 'response' || !frame.requestId) {
      return
    }

    const pending = this.pending.get(frame.requestId)
    if (!pending) {
      return
    }

    globalThis.clearTimeout(pending.timeout)
    this.pending.delete(frame.requestId)

    if (frame.ok) {
      pending.resolve(frame.result ?? {})
      return
    }

    pending.reject(
      new ControlPlaneClientError(
        frame.error?.message ?? 'Firmware returned an unsuccessful USB response.',
        frame.error?.code ?? 'usb_error',
        frame.error?.retryable ?? true,
        frame.error?.details
      )
    )
  }

  private rejectAll(error: Error) {
    for (const [requestId, pending] of this.pending) {
      globalThis.clearTimeout(pending.timeout)
      pending.reject(error)
      this.pending.delete(requestId)
    }
  }

  private observeDiagnostic(diagnostic: WebSerialDiagnostic) {
    if (diagnostic.kind === 'boot_stage' && diagnostic.reason === 'runtime_ready') {
      this.runtimeReadyObserved = true
      for (const waiter of this.runtimeReadyWaiters) {
        globalThis.clearTimeout(waiter.timeout)
        waiter.resolve()
      }
      this.runtimeReadyWaiters.clear()
    }
    this.onDiagnostic?.(diagnostic)
  }

  private waitForRuntimeReady(deadline: number) {
    if (this.runtimeReadyObserved) return Promise.resolve()
    const remaining = deadline - Date.now()
    if (remaining <= 0) return Promise.reject(runtimeInitializationTimeoutError())

    return new Promise<void>((resolve, reject) => {
      const waiter: RuntimeReadyWaiter = {
        resolve: () => {
          globalThis.clearTimeout(waiter.timeout)
          this.runtimeReadyWaiters.delete(waiter)
          resolve()
        },
        reject: (error) => {
          globalThis.clearTimeout(waiter.timeout)
          this.runtimeReadyWaiters.delete(waiter)
          reject(error)
        },
        timeout: undefined,
      }
      waiter.timeout = globalThis.setTimeout(
        () => waiter.reject(runtimeInitializationTimeoutError()),
        remaining
      )
      this.runtimeReadyWaiters.add(waiter)
    })
  }

  private rejectRuntimeReadyWaiters(error: Error) {
    for (const waiter of this.runtimeReadyWaiters) {
      waiter.reject(error)
    }
    this.runtimeReadyWaiters.clear()
  }
}

function parseUsbResponseWire(line: string): UsbResponseWire | null {
  const trimmed = line.trim()
  if (!trimmed) return null

  const tryParse = (candidate: string): UsbResponseWire | null => {
    try {
      return JSON.parse(candidate) as UsbResponseWire
    } catch {
      return null
    }
  }

  const direct = tryParse(trimmed)
  if (direct) return direct

  // USB Serial/JTAG multiplexes diagnostic bytes with the JSONL control
  // channel. Match the established Web Serial transport by recovering the
  // bounded JSON object while still requiring its requestId below.
  const firstBrace = trimmed.indexOf('{')
  const lastBrace = trimmed.lastIndexOf('}')
  if (firstBrace < 0 || lastBrace <= firstBrace) return null
  return tryParse(trimmed.slice(firstBrace, lastBrace + 1))
}

export function parseWebSerialDiagnostic(line: string): WebSerialDiagnostic | null {
  const bootStage = line.match(/^boot_stage=([a-z0-9_]+)$/)?.[1]
  if (bootStage) {
    return { kind: 'boot_stage', reason: bootStage }
  }
  const resetReason = line.match(/^reset_reason=([a-z0-9_]+)$/)?.[1]
  if (resetReason) {
    return { kind: 'reset', reason: resetReason }
  }
  const panicReason = line.match(/^panic=([a-z0-9_]+)$/)?.[1]
  if (panicReason) {
    return { kind: 'panic', reason: panicReason }
  }
  return null
}

export function formatWebSerialEventTime(date: Date) {
  return [date.getHours(), date.getMinutes(), date.getSeconds()]
    .map((value) => String(value).padStart(2, '0'))
    .join(':')
}

async function withCleanupTimeout(operation: Promise<unknown> | undefined) {
  if (!operation) return
  await Promise.race([
    operation,
    new Promise<void>((resolve) => globalThis.setTimeout(resolve, 500)),
  ])
}

async function closeBrowserSerialPort(port: BrowserSerialPort) {
  const deadline = Date.now() + WEB_SERIAL_CLOSE_TIMEOUT_MS
  for (;;) {
    try {
      await port.close()
      return
    } catch (error) {
      if (Date.now() >= deadline) throw normalizeBrowserSerialError(error)
      await new Promise<void>((resolve) =>
        globalThis.setTimeout(resolve, Math.min(WEB_SERIAL_CLOSE_RETRY_MS, deadline - Date.now()))
      )
    }
  }
}

function createUsbRequestFrame(op: UsbRequestFrame['op']) {
  return (requestId: string): UsbRequestFrame => ({
    type: 'request',
    requestId,
    op,
  })
}

function createWebSerialRequestId() {
  const random = Math.random().toString(16).slice(2, 8)
  return `web-${Date.now()}-${random}`
}

export function normalizeBrowserSerialError(error: unknown) {
  if (error instanceof ControlPlaneClientError) {
    return error
  }

  if (error instanceof Error && /no port selected by (the )?user/i.test(error.message)) {
    return new ControlPlaneClientError(
      '浏览器未确认串口设备。请重新选择 Flux Purr USB JTAG/serial 设备。',
      'web_serial_port_not_selected',
      true
    )
  }

  if (error instanceof Error && /must be handling a user gesture/i.test(error.message)) {
    return new ControlPlaneClientError(
      '浏览器 USB 选择器未在用户点击时打开。请重新点击“运行预检”。',
      'web_serial_user_gesture_required',
      true
    )
  }

  return new ControlPlaneClientError(
    error instanceof Error ? error.message : 'Web Serial read failed.',
    'web_serial_read_failed',
    true
  )
}

function runtimeInitializationTimeoutError() {
  return new ControlPlaneClientError(
    'Flux Purr 运行时在 8 秒内未响应；空片、外来固件或 ROM 模式设备请切换“安装或恢复”。',
    'web_serial_runtime_not_ready',
    true
  )
}

function isFirmwareInitializationPending(error: unknown) {
  return (
    (error instanceof ControlPlaneClientError &&
      (error.code === 'usb_response_timeout' || error.code === 'startup_busy')) ||
    (error instanceof Error &&
      /not available until memory and WiFi initialization completes/i.test(error.message))
  )
}

function mergeCapabilities(...capabilitySets: string[][]) {
  return Array.from(new Set(capabilitySets.flat()))
}

function formatUptime(seconds: number) {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const rest = seconds % 60
  return [hours, minutes, rest].map((value) => String(value).padStart(2, '0')).join(':')
}
