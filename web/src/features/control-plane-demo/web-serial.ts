import type {
  ApiErrorEnvelope,
  ControlPlaneStatus,
  DirectRuntimeConfigRequest,
  Identity,
  NetworkSummary,
  UsbRequestFrame,
  UsbRuntimeConfigFrame,
} from './contracts'
import { ControlPlaneClientError } from './transport-client'
import type { DeviceTarget } from './types'

const WEB_SERIAL_BAUD_RATE = 115_200
const WEB_SERIAL_RPC_TIMEOUT_MS = 12_000
const WEB_SERIAL_DEVICE_BASE_URL = 'webserial://selected'
const WEB_SERIAL_LINE_LIMIT = 2048

export type WebSerialConnectionState = 'unsupported' | 'idle' | 'connecting' | 'connected' | 'error'

export interface BrowserSerial {
  requestPort(options?: unknown): Promise<BrowserSerialPort>
}

export interface BrowserSerialPort {
  readable: ReadableStream<Uint8Array> | null
  writable: WritableStream<Uint8Array> | null
  open(options: { baudRate: number; bufferSize?: number }): Promise<void>
  close(): Promise<void>
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
    voltageMv: probe.status.voltageMv,
    currentMa: probe.status.currentMa,
    pdRequestMv: probe.status.pdRequestMv,
    pdContractMv: probe.status.pdContractMv,
    pdState: probe.status.pdState,
    heaterOutputPercent: probe.status.heaterOutputPercent,
    activeCoolingEnabled: probe.status.activeCoolingEnabled,
    fanState: probe.status.fanDisplayState,
    wifiRssi: probe.network.wifiRssi ?? null,
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
  private port: BrowserSerialPort | null = null
  private reader: ReadableStreamDefaultReader<Uint8Array> | null = null
  private lineBuffer = ''
  private readPump: Promise<void> | null = null
  private writeChain = Promise.resolve()

  constructor({
    serial = getBrowserSerial(),
    baudRate = WEB_SERIAL_BAUD_RATE,
  }: {
    serial?: BrowserSerial | null
    baudRate?: number
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
  }

  async connect() {
    const port = await this.serial.requestPort()
    await port.open({ baudRate: this.baudRate })
    this.port = port
    this.readPump = this.readLoop()
    return this.probe()
  }

  async disconnect() {
    const port = this.port
    this.port = null
    this.rejectAll(
      new ControlPlaneClientError('Web Serial connection closed.', 'web_serial_closed', true)
    )
    await this.reader?.cancel().catch(() => undefined)
    await this.readPump?.catch(() => undefined)
    if (port) {
      await port.close().catch(() => undefined)
    }
  }

  async probe(): Promise<WebSerialProbe> {
    const identity = await this.requestPayload<Identity>(
      'identity',
      createUsbRequestFrame('get_identity')
    )
    const network = await this.requestPayload<NetworkSummary>(
      'network',
      createUsbRequestFrame('get_network')
    )
    const status = await this.requestPayload<ControlPlaneStatus>(
      'status',
      createUsbRequestFrame('get_status')
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

  private async requestPayload<T>(
    payloadKey: string,
    frameFactory: (requestId: string) => UsbRequestFrame | UsbRuntimeConfigFrame
  ): Promise<T> {
    const requestId = createWebSerialRequestId()
    const result = await this.exchange(frameFactory(requestId))
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

  private exchange(frame: UsbRequestFrame | UsbRuntimeConfigFrame) {
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
      }, WEB_SERIAL_RPC_TIMEOUT_MS)
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
        const wrappedError = serialError(error)
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
          break
        }
        if (value) {
          this.consumeSerialText(this.decoder.decode(value, { stream: true }))
        }
      }
    } catch (error) {
      this.rejectAll(serialError(error))
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

    let frame: UsbResponseWire
    try {
      frame = JSON.parse(line) as UsbResponseWire
    } catch {
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

function serialError(error: unknown) {
  if (error instanceof ControlPlaneClientError) {
    return error
  }

  return new ControlPlaneClientError(
    error instanceof Error ? error.message : 'Web Serial read failed.',
    'web_serial_read_failed',
    true
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
