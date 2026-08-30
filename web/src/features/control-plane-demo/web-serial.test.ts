import { describe, expect, it, vi } from 'vitest'
import type { BrowserSerial, BrowserSerialPort } from './web-serial'
import {
  formatWebSerialEventTime,
  isDirectWebSerialDevice,
  normalizeBrowserSerialError,
  selectBrowserSerialPort,
  WEB_SERIAL_INITIALIZATION_TIMEOUT_MS,
  WebSerialControlPlaneClient,
  webSerialProbeToDeviceTarget,
} from './web-serial'

describe('web serial control-plane client', () => {
  it('formats browser-originated trace events with a local clock time', () => {
    expect(formatWebSerialEventTime(new Date(2026, 0, 1, 7, 5, 9))).toBe('07:05:09')
  })

  it('probes firmware over USB JSONL and maps the direct device target', async () => {
    const fake = new FakeSerial()
    const client = new WebSerialControlPlaneClient({ serial: fake })

    const probe = await client.connect()
    const target = webSerialProbeToDeviceTarget(probe)

    expect(
      JSON.stringify(responseFor({ type: 'request', op: 'get_status' })).length
    ).toBeGreaterThan(2_048)

    expect(fake.requests.map((request) => request.op)).toEqual([
      'get_identity',
      'get_network',
      'get_status',
    ])
    expect(target).toMatchObject({
      id: 'web-serial-flux-purr-s3-001',
      transport: 'serial',
      baseUrl: 'webserial://selected',
      leaseState: 'active',
      currentTempC: 181.5,
      targetTempC: 220,
      selectedPresetIndex: 7,
      presetsC: [50, 100, 120, null, 180, 200, 210, 220, 250, 300],
      networkState: 'connected',
      configurationGeneration: 3,
      transitionSequence: 26,
      wifiFailureCode: null,
    })
    expect(target.capabilities).toContain('usb_jsonl')
    expect(isDirectWebSerialDevice(target)).toBe(true)

    await client.disconnect()
  })

  it('reuses one browser-authorized port before opening the chooser', async () => {
    const serial = new AuthorizedSerial()
    const client = new WebSerialControlPlaneClient({
      serial,
      preauthorizedPorts: [serial.authorizedPort],
    })

    await client.connect()

    expect(serial.requestPortCalls).toBe(0)
    expect(serial.authorizedPortOpened).toBe(true)
    expect(serial.authorizedPort.signalHistory).toEqual([])
    await client.disconnect()
  })

  it('recovers a matching USB JSONL response that follows diagnostic bytes', async () => {
    const noisy = new FakeSerial('boot_stage=runtime_ready ')
    const client = new WebSerialControlPlaneClient({ serial: noisy })

    await expect(client.connect()).resolves.toMatchObject({
      identity: { deviceId: 'flux-purr-s3-001' },
    })

    await client.disconnect()
  })

  it('retries USB startup exactly once after firmware reports runtime_ready', async () => {
    const diagnostics: string[] = []
    const retries: number[] = []
    const serial = new StartupGatedSerial()
    const client = new WebSerialControlPlaneClient({
      serial,
      onDiagnostic: (diagnostic) => diagnostics.push(`${diagnostic.kind}:${diagnostic.reason}`),
      onInitializationRetry: ({ attempt }) => retries.push(attempt),
    })

    await expect(client.connect()).resolves.toMatchObject({
      identity: { deviceId: 'flux-purr-s3-001' },
    })

    expect(serial.requests.map((request) => request.op)).toEqual([
      'get_identity',
      'get_identity',
      'get_network',
      'get_status',
    ])
    expect(retries).toEqual([1])
    expect(diagnostics).toContain('boot_stage:runtime_ready')
    await client.disconnect()
  })

  it('retries a transient close before handing the same authorized port to the next client', async () => {
    const serial = new RetriableCloseSerial()
    const firstClient = new WebSerialControlPlaneClient({
      serial,
      preauthorizedPorts: [serial.authorizedPort],
    })
    await firstClient.connect()
    await firstClient.disconnect()

    const nextClient = new WebSerialControlPlaneClient({
      serial,
      preauthorizedPorts: [serial.authorizedPort],
    })
    await expect(nextClient.connect()).resolves.toBeDefined()
    expect(serial.authorizedPort.closeCalls).toBe(2)
    await nextClient.disconnect()
  })

  it('opens the chooser when Add device explicitly requests another port', async () => {
    const serial = new AuthorizedSerial()

    const selected = await selectBrowserSerialPort(serial, [serial.authorizedPort], true)

    expect(selected).toBe(serial.authorizedPort)
    expect(serial.requestPortCalls).toBe(1)
  })

  it('opens the first-time chooser synchronously instead of awaiting authorized-port discovery', async () => {
    const serial = new FirstAuthorizationSerial()
    const client = new WebSerialControlPlaneClient({ serial })

    const connection = client.connect()

    expect(serial.requestPortCalls).toBe(1)
    expect(serial.getPortsCalls).toBe(0)
    await connection
    await client.disconnect()
  })

  it('does not open the chooser during preauthorized-only recovery', async () => {
    const serial = new AuthorizedSerial()
    const client = new WebSerialControlPlaneClient({
      serial,
      preauthorizedPorts: [],
      requestPortWhenUnavailable: false,
    })

    await expect(client.connect()).rejects.toMatchObject({ code: 'web_serial_port_required' })
    expect(serial.requestPortCalls).toBe(0)
  })

  it('explains when the browser serial chooser closes without a selected port', async () => {
    const client = new WebSerialControlPlaneClient({
      serial: new CancelledChooserSerial(),
    })

    await expect(client.connect()).rejects.toMatchObject({
      code: 'web_serial_port_not_selected',
      message: '浏览器未确认串口设备。请重新选择 Flux Purr USB JTAG/serial 设备。',
    })
  })

  it('normalizes cancellation from the forced Add device chooser path', () => {
    expect(normalizeBrowserSerialError(new Error('No port selected by the user.'))).toMatchObject({
      code: 'web_serial_port_not_selected',
      message: '浏览器未确认串口设备。请重新选择 Flux Purr USB JTAG/serial 设备。',
    })
  })

  it('explains a chooser request made outside the browser user gesture', () => {
    expect(
      normalizeBrowserSerialError(
        new Error("Failed to execute 'requestPort' on 'Serial': Must be handling a user gesture")
      )
    ).toMatchObject({
      code: 'web_serial_user_gesture_required',
      message: '浏览器 USB 选择器未在用户点击时打开。请重新点击“运行预检”。',
    })
  })

  it('sends direct runtime_config frames and returns the firmware status payload', async () => {
    const fake = new FakeSerial()
    const client = new WebSerialControlPlaneClient({ serial: fake })
    await client.connect()

    const status = await client.configureRuntime({
      targetTempC: 235,
      selectedPresetSlot: 3,
      presetsC: [50, 100, 120, 235, 180, 200, 210, 220, 250, 300],
      activeCoolingEnabled: false,
      heaterEnabled: false,
    })

    expect(status).toMatchObject({
      targetTempC: 235,
      selectedPresetSlot: 3,
      presetsC: [50, 100, 120, 235, 180, 200, 210, 220, 250, 300],
      activeCoolingEnabled: false,
      heaterEnabled: false,
      fanDisplayState: 'OFF',
    })
    expect(fake.requests.at(-1)).toMatchObject({
      type: 'runtime_config',
      targetTempC: 235,
      selectedPresetSlot: 3,
      presetsC: [50, 100, 120, 235, 180, 200, 210, 220, 250, 300],
      activeCoolingEnabled: false,
      heaterEnabled: false,
    })

    await client.disconnect()
  })

  it('sends WiFi config frames over Web Serial and returns the network payload', async () => {
    const fake = new FakeSerial()
    const client = new WebSerialControlPlaneClient({ serial: fake })
    await client.connect()

    const network = await client.configureWifi({
      op: 'set',
      ssid: 'FluxPurr-Lab',
      password: 'secret-pass',
    })

    expect(network).toMatchObject({ state: 'saving', ssid: 'FluxPurr-Lab' })
    expect(fake.requests.at(-1)).toMatchObject({
      type: 'wifi_config',
      op: 'set',
      ssid: 'FluxPurr-Lab',
      password: 'secret-pass',
    })

    await client.configureWifi({ op: 'clear' })
    expect(fake.requests.at(-1)).toMatchObject({ type: 'wifi_config', op: 'clear' })

    await client.configureWifi({ op: 'cancel' })
    expect(fake.requests.at(-1)).toMatchObject({ type: 'wifi_config', op: 'cancel' })

    await client.getNetwork()
    expect(fake.requests.at(-1)).toMatchObject({ type: 'request', op: 'get_network' })
    await client.disconnect()
  })

  it('settles an in-flight WiFi write as soon as the serial stream closes', async () => {
    const fake = new FakeSerial('', (request) =>
      request.type === 'wifi_config' ? null : responseFor(request)
    )
    const client = new WebSerialControlPlaneClient({ serial: fake })
    await client.connect()

    const pending = client.configureWifi({
      op: 'set',
      ssid: 'FluxPurr-Lab',
      password: 'secret-pass',
    })
    await vi.waitFor(() => {
      expect(fake.requests.at(-1)).toMatchObject({ type: 'wifi_config', op: 'set' })
    })

    fake.closeReadable()

    await expect(pending).rejects.toMatchObject({ code: 'web_serial_stream_closed' })
    await client.disconnect()
  })

  it('reports firmware boot and reset markers without treating arbitrary serial output as device state', async () => {
    const fake = new FakeSerial()
    const diagnostics: string[] = []
    const client = new WebSerialControlPlaneClient({
      serial: fake,
      onDiagnostic: (diagnostic) => diagnostics.push(`${diagnostic.kind}:${diagnostic.reason}`),
    })
    await client.connect()

    fake.emitLine('boot chatter that is not part of the control-plane contract')
    fake.emitLine('boot_stage=runtime_ready')
    fake.emitLine('reset_reason=system_brownout')
    fake.emitLine('panic=firmware_fault')
    await new Promise<void>((resolve) => setTimeout(resolve, 0))

    expect(diagnostics).toEqual([
      'boot_stage:runtime_ready',
      'reset:system_brownout',
      'panic:firmware_fault',
    ])
    await client.disconnect()
  })

  it('closes a port that resolves after a cancelled connection attempt', async () => {
    const serial = new DeferredSerial()
    const client = new WebSerialControlPlaneClient({ serial })
    const connection = client.connect()

    await client.disconnect()
    const port = new DeferredSerialPort()
    serial.resolve(port)

    await expect(connection).rejects.toMatchObject({ code: 'web_serial_closed' })
    expect(port.closed).toBe(true)
  })

  it('closes a port when opening fails so the next attempt can retry it', async () => {
    const serial = new OpenFailureSerial()
    const firstClient = new WebSerialControlPlaneClient({ serial })

    await expect(firstClient.connect()).rejects.toMatchObject({
      code: 'web_serial_read_failed',
      message: 'Failed to open serial port.',
    })
    expect(serial.port.closeCalls).toBe(1)

    const nextClient = new WebSerialControlPlaneClient({ serial })
    await expect(nextClient.connect()).resolves.toMatchObject({
      identity: { deviceId: 'flux-purr-s3-001' },
    })
    await nextClient.disconnect()
  })

  it('bounds a non-Flux runtime probe and closes the port', async () => {
    vi.useFakeTimers()
    try {
      expect(WEB_SERIAL_INITIALIZATION_TIMEOUT_MS).toBe(8_000)
      const serial = new SilentSerial()
      const client = new WebSerialControlPlaneClient({ serial })
      const connection = client.connect()
      const failure = expect(connection).rejects.toMatchObject({
        code: 'web_serial_runtime_not_ready',
        message:
          'Flux Purr 运行时在 8 秒内未响应；空片、外来固件或 ROM 模式设备请切换“安装或恢复”。',
      })

      await vi.advanceTimersByTimeAsync(WEB_SERIAL_INITIALIZATION_TIMEOUT_MS + 1_000)

      await failure
      expect(serial.port.closed).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })

  it('sends calibration auto-job frames over USB JSONL', async () => {
    const fake = new FakeSerial()
    const client = new WebSerialControlPlaneClient({ serial: fake })
    await client.connect()

    const current = await client.getCalibrationJob()
    const started = await client.configureCalibrationJob({
      op: 'start',
      kind: 'vin_adc_auto',
    })

    expect(current).toMatchObject({
      kind: null,
      status: 'idle',
      progressPercent: 0,
    })
    expect(started).toMatchObject({
      kind: 'vin_adc_auto',
      status: 'running',
      nextRequestMv: 11000,
    })
    expect(fake.requests.at(-1)).toMatchObject({
      type: 'calibration_job',
      op: 'start',
      kind: 'vin_adc_auto',
    })

    await client.disconnect()
  })

  it('sends RTD calibration samples with operator temperature and target ADC', async () => {
    const fake = new FakeSerial()
    const client = new WebSerialControlPlaneClient({ serial: fake })
    await client.connect()

    await client.configureCalibration({
      op: 'capture',
      channel: 'rtd_adc',
      referenceTempC: 21.6,
      targetAdcMv: 970,
    })

    expect(fake.requests.at(-1)).toMatchObject({
      type: 'calibration_config',
      op: 'capture',
      channel: 'rtd_adc',
      referenceTempC: 21.6,
      targetAdcMv: 970,
    })

    await client.disconnect()
  })

  it('reads the thermal plant run with its paging cursor and cooling trace', async () => {
    const fake = new FakeSerial()
    const client = new WebSerialControlPlaneClient({ serial: fake })
    await client.connect()

    const snapshot = await client.getThermalPlantRun(16)

    expect(fake.requests.at(-1)).toMatchObject({
      type: 'thermal_plant_run',
      afterSample: 16,
    })
    expect(snapshot.tracePage).toMatchObject({
      startSample: 16,
      points: [{ sampleIndex: 16, phase: 'cooling' }],
    })

    await client.disconnect()
  })

  it('uses the firmware thermal tuning frame for both paged reads and commands', async () => {
    const fake = new FakeSerial()
    const client = new WebSerialControlPlaneClient({ serial: fake })
    await client.connect()

    const current = await client.getThermalTuningRun(7, 24)
    const started = await client.configureThermalTuningRun({
      op: 'start',
      powerClass: 'pps5a',
    })

    expect(current).toMatchObject({ schema: 'thermal_tuning_run_v1' })
    expect(started.run.powerClass).toBe('pps5a')
    expect(fake.requests.at(-2)).toMatchObject({
      type: 'thermal_tuning_run',
      op: 'get',
      afterSequence: 7,
      limit: 24,
    })
    expect(fake.requests.at(-1)).toMatchObject({
      type: 'thermal_tuning_run',
      op: 'start',
      powerClass: 'pps5a',
    })

    await client.disconnect()
  })

  it('omits the exclusive thermal tuning cursor for the first page', async () => {
    const fake = new FakeSerial()
    const client = new WebSerialControlPlaneClient({ serial: fake })
    await client.connect()

    await client.getThermalTuningRun()

    expect(fake.requests.at(-1)).toMatchObject({
      type: 'thermal_tuning_run',
      op: 'get',
      limit: 16,
    })
    expect(fake.requests.at(-1)).not.toHaveProperty('afterSequence')

    await client.disconnect()
  })
})

class FakeSerial implements BrowserSerial {
  readonly requests: Array<Record<string, unknown>> = []
  private readonly port: FakeSerialPort

  constructor(
    responsePrefix = '',
    responseForRequest: (
      request: Record<string, unknown>
    ) => Record<string, unknown> | null = responseFor
  ) {
    this.port = new FakeSerialPort(this.requests, undefined, responsePrefix, responseForRequest)
  }

  requestPort(): Promise<BrowserSerialPort> {
    return Promise.resolve(this.port)
  }

  emitLine(line: string) {
    this.port.emitLine(line)
  }

  closeReadable() {
    this.port.closeReadable()
  }
}

class StartupGatedSerial implements BrowserSerial {
  readonly requests: Array<Record<string, unknown>> = []
  private readonly port = new StartupGatedSerialPort(this.requests)

  requestPort(): Promise<BrowserSerialPort> {
    return Promise.resolve(this.port)
  }
}

class DeferredSerial implements BrowserSerial {
  private resolvePort!: (port: BrowserSerialPort) => void
  private readonly portPromise = new Promise<BrowserSerialPort>((resolve) => {
    this.resolvePort = resolve
  })

  requestPort(): Promise<BrowserSerialPort> {
    return this.portPromise
  }

  resolve(port: BrowserSerialPort) {
    this.resolvePort(port)
  }
}

class AuthorizedSerial implements BrowserSerial {
  requestPortCalls = 0
  authorizedPortOpened = false
  readonly authorizedPort = new FakeSerialPort([], () => {
    this.authorizedPortOpened = true
  })

  getPorts(): Promise<BrowserSerialPort[]> {
    return Promise.resolve([this.authorizedPort])
  }

  requestPort(): Promise<BrowserSerialPort> {
    this.requestPortCalls += 1
    return Promise.resolve(this.authorizedPort)
  }
}

class FirstAuthorizationSerial extends FakeSerial {
  requestPortCalls = 0
  getPortsCalls = 0

  getPorts(): Promise<BrowserSerialPort[]> {
    this.getPortsCalls += 1
    return Promise.resolve([])
  }

  requestPort(): Promise<BrowserSerialPort> {
    this.requestPortCalls += 1
    return super.requestPort()
  }
}

class CancelledChooserSerial implements BrowserSerial {
  requestPort(): Promise<BrowserSerialPort> {
    return Promise.reject(new Error('No port selected by the user.'))
  }
}

class DeferredSerialPort implements BrowserSerialPort {
  readonly readable = null
  readonly writable = null
  closed = false

  open(): Promise<void> {
    return Promise.resolve()
  }

  close(): Promise<void> {
    this.closed = true
    return Promise.resolve()
  }
}

class SilentSerial implements BrowserSerial {
  readonly port = new SilentSerialPort()

  requestPort(): Promise<BrowserSerialPort> {
    return Promise.resolve(this.port)
  }
}

class SilentSerialPort implements BrowserSerialPort {
  readonly readable = new ReadableStream<Uint8Array>()
  readonly writable = new WritableStream<Uint8Array>()
  closed = false

  open(): Promise<void> {
    return Promise.resolve()
  }

  close(): Promise<void> {
    this.closed = true
    return Promise.resolve()
  }
}

class FakeSerialPort implements BrowserSerialPort {
  readonly readable: ReadableStream<Uint8Array>
  readonly writable: WritableStream<Uint8Array>
  private controller: ReadableStreamDefaultController<Uint8Array> | null = null
  private readonly decoder = new TextDecoder()
  private readonly encoder = new TextEncoder()
  private readonly requests: Array<Record<string, unknown>>
  private readonly onOpen?: () => void
  private readonly responsePrefix: string
  private readonly responseForRequest: (
    request: Record<string, unknown>
  ) => Record<string, unknown> | null
  private writeBuffer = ''
  signalHistory: Array<{ dataTerminalReady?: boolean; requestToSend?: boolean }> = []

  constructor(
    requests: Array<Record<string, unknown>>,
    onOpen?: () => void,
    responsePrefix = '',
    responseForRequest: (
      request: Record<string, unknown>
    ) => Record<string, unknown> | null = responseFor
  ) {
    this.requests = requests
    this.onOpen = onOpen
    this.responsePrefix = responsePrefix
    this.responseForRequest = responseForRequest
    this.readable = new ReadableStream<Uint8Array>({
      start: (controller) => {
        this.controller = controller
      },
    })
    this.writable = new WritableStream<Uint8Array>({
      write: (chunk) => {
        this.writeBuffer += this.decoder.decode(chunk, { stream: true })
        this.flushRequests()
      },
    })
  }

  open(): Promise<void> {
    this.onOpen?.()
    return Promise.resolve()
  }

  setSignals(signals: { dataTerminalReady?: boolean; requestToSend?: boolean }): Promise<void> {
    this.signalHistory.push(signals)
    return Promise.resolve()
  }

  close(): Promise<void> {
    return Promise.resolve()
  }

  emitLine(line: string) {
    this.controller?.enqueue(this.encoder.encode(`${line}\n`))
  }

  closeReadable() {
    this.controller?.close()
  }

  private flushRequests() {
    let newlineIndex = this.writeBuffer.indexOf('\n')
    while (newlineIndex >= 0) {
      const line = this.writeBuffer.slice(0, newlineIndex)
      this.writeBuffer = this.writeBuffer.slice(newlineIndex + 1)
      const request = JSON.parse(line) as Record<string, unknown>
      this.requests.push(request)
      const response = this.responseForRequest(request)
      if (response) {
        this.controller?.enqueue(
          this.encoder.encode(`${this.responsePrefix}${JSON.stringify(response)}\n`)
        )
      }
      newlineIndex = this.writeBuffer.indexOf('\n')
    }
  }
}

class OpenFailureSerial implements BrowserSerial {
  readonly port = new OpenFailureSerialPort([])

  requestPort(): Promise<BrowserSerialPort> {
    return Promise.resolve(this.port)
  }
}

class OpenFailureSerialPort extends FakeSerialPort {
  closeCalls = 0
  private failNextOpen = true

  open(): Promise<void> {
    if (this.failNextOpen) {
      this.failNextOpen = false
      return Promise.reject(new Error('Failed to open serial port.'))
    }
    return super.open()
  }

  close(): Promise<void> {
    this.closeCalls += 1
    return super.close()
  }
}

class StartupGatedSerialPort implements BrowserSerialPort {
  readonly readable: ReadableStream<Uint8Array>
  readonly writable: WritableStream<Uint8Array>
  private readonly decoder = new TextDecoder()
  private readonly encoder = new TextEncoder()
  private readonly requests: Array<Record<string, unknown>>
  private controller: ReadableStreamDefaultController<Uint8Array> | null = null
  private writeBuffer = ''
  private firstIdentity = true

  constructor(requests: Array<Record<string, unknown>>) {
    this.requests = requests
    this.readable = new ReadableStream<Uint8Array>({
      start: (controller) => {
        this.controller = controller
      },
    })
    this.writable = new WritableStream<Uint8Array>({
      write: (chunk) => {
        this.writeBuffer += this.decoder.decode(chunk, { stream: true })
        this.flushRequests()
      },
    })
  }

  open(): Promise<void> {
    return Promise.resolve()
  }

  close(): Promise<void> {
    return Promise.resolve()
  }

  private flushRequests() {
    let newlineIndex = this.writeBuffer.indexOf('\n')
    while (newlineIndex >= 0) {
      const line = this.writeBuffer.slice(0, newlineIndex)
      this.writeBuffer = this.writeBuffer.slice(newlineIndex + 1)
      const request = JSON.parse(line) as Record<string, unknown>
      this.requests.push(request)
      if (request.op === 'get_identity' && this.firstIdentity) {
        this.firstIdentity = false
        this.emit({
          type: 'response',
          requestId: request.requestId,
          ok: false,
          error: {
            code: 'startup_busy',
            message: 'Runtime is not available until memory and WiFi initialization completes.',
            retryable: true,
          },
        })
        this.emit('boot_stage=runtime_ready')
      } else {
        this.emit(responseFor(request))
      }
      newlineIndex = this.writeBuffer.indexOf('\n')
    }
  }

  private emit(message: Record<string, unknown> | string) {
    const line = typeof message === 'string' ? message : JSON.stringify(message)
    this.controller?.enqueue(this.encoder.encode(`${line}\n`))
  }
}

class RetriableCloseSerial implements BrowserSerial {
  readonly authorizedPort = new RetriableCloseSerialPort()

  getPorts(): Promise<BrowserSerialPort[]> {
    return Promise.resolve([this.authorizedPort])
  }

  requestPort(): Promise<BrowserSerialPort> {
    return Promise.resolve(this.authorizedPort)
  }
}

class RetriableCloseSerialPort implements BrowserSerialPort {
  closeCalls = 0
  private openState = false
  private controller: ReadableStreamDefaultController<Uint8Array> | null = null
  private readonly decoder = new TextDecoder()
  private readonly encoder = new TextEncoder()
  private writeBuffer = ''
  readable: ReadableStream<Uint8Array> | null = null
  writable: WritableStream<Uint8Array> | null = null

  constructor() {
    this.createStreams()
  }

  open(): Promise<void> {
    if (this.openState)
      return Promise.reject(new DOMException('Failed to open serial port.', 'InvalidStateError'))
    this.openState = true
    this.createStreams()
    return Promise.resolve()
  }

  close(): Promise<void> {
    this.closeCalls += 1
    if (this.closeCalls === 1) {
      return Promise.reject(new DOMException('Port is still closing.', 'NetworkError'))
    }
    this.openState = false
    return Promise.resolve()
  }

  private createStreams() {
    this.writeBuffer = ''
    this.readable = new ReadableStream<Uint8Array>({
      start: (controller) => {
        this.controller = controller
      },
    })
    this.writable = new WritableStream<Uint8Array>({
      write: (chunk) => {
        this.writeBuffer += this.decoder.decode(chunk, { stream: true })
        let newlineIndex = this.writeBuffer.indexOf('\n')
        while (newlineIndex >= 0) {
          const line = this.writeBuffer.slice(0, newlineIndex)
          this.writeBuffer = this.writeBuffer.slice(newlineIndex + 1)
          const request = JSON.parse(line) as Record<string, unknown>
          this.controller?.enqueue(this.encoder.encode(`${JSON.stringify(responseFor(request))}\n`))
          newlineIndex = this.writeBuffer.indexOf('\n')
        }
      },
    })
  }
}

function responseFor(request: Record<string, unknown>) {
  const requestId = request.requestId
  if (request.type === 'request' && request.op === 'get_identity') {
    return { type: 'response', requestId, ok: true, result: { identity } }
  }
  if (request.type === 'request' && request.op === 'get_network') {
    return { type: 'response', requestId, ok: true, result: { network } }
  }
  if (request.type === 'request' && request.op === 'get_status') {
    return { type: 'response', requestId, ok: true, result: { status: baseStatus } }
  }
  if (request.type === 'request' && request.op === 'get_calibration_job') {
    return {
      type: 'response',
      requestId,
      ok: true,
      result: {
        calibration_job: {
          kind: null,
          status: 'idle',
          progressPercent: 0,
          samplesCollected: 0,
          nextRequestMv: null,
          message: null,
        },
      },
    }
  }
  if (request.type === 'thermal_plant_run') {
    return {
      type: 'response',
      requestId,
      ok: true,
      result: {
        thermal_plant_run: {
          version: 1,
          attempt: {
            runId: 9,
            status: 'running',
            phase: 'cooling',
            progressPercent: 72,
            elapsedMs: 120_000,
            currentTempCentiC: 8000,
            heaterVoltageMv: 0,
            dutyPercent: 0,
            sampleCount: 17,
            restartAllowed: false,
            error: null,
          },
          tracePage: {
            startSample: request.afterSample ?? 0,
            nextSample: null,
            totalSamples: 17,
            points: [
              {
                sampleIndex: request.afterSample ?? 0,
                elapsedMs: 120_000,
                temperatureCentiC: 8000,
                heaterVoltageMv: 0,
                dutyPercent: 0,
                phase: 'cooling',
              },
            ],
          },
          provisionalCurve: null,
          activeResult: null,
        },
      },
    }
  }
  if (request.type === 'thermal_tuning_run') {
    const powerClass = request.powerClass === 'pps5a' ? 'pps5a' : 'pps3a'
    return {
      type: 'response',
      requestId,
      ok: true,
      result: {
        thermal_tuning_run: {
          schema: 'thermal_tuning_run_v1',
          run: {
            runId: 'mock-tuning-serial-1',
            state: request.op === 'start' ? 'running' : 'idle',
            powerClass: request.op === 'start' ? powerClass : null,
            phase: request.op === 'start' ? 'scout' : 'idle',
            currentTargetC: request.op === 'start' ? 60 : null,
            targetProgress: { acceptedC: [], failedC: [], skippedC: [] },
            terminalDisposition: null,
            eligibility: { ready: true, reasons: [], activeOwner: null },
            review: {
              state: request.op === 'start' ? 'recording' : 'not_applicable',
              reason: null,
              acknowledgedThrough: 0,
              terminalSequence: null,
              traceDigest: null,
            },
            candidate: {
              candidateId: null,
              candidateHash: null,
              powerClass: request.op === 'start' ? powerClass : null,
              promotionState: request.op === 'start' ? 'awaiting_review' : 'unavailable',
            },
            journal: { lastRunId: null, lastDisposition: null },
          },
          page: {
            earliestSequence: 1,
            emittedThrough: 0,
            nextAfterSequence: 1,
            acknowledgedThrough: 0,
            digestThroughPage: null,
            events: [],
          },
        },
      },
    }
  }
  if (request.type === 'runtime_config') {
    const selectedPresetSlot =
      typeof request.selectedPresetSlot === 'number'
        ? request.selectedPresetSlot
        : baseStatus.selectedPresetSlot
    const presetsC = Array.isArray(request.presetsC) ? request.presetsC : baseStatus.presetsC
    const selectedPresetTemp = presetsC[selectedPresetSlot]
    return {
      type: 'response',
      requestId,
      ok: true,
      result: {
        status: {
          ...baseStatus,
          targetTempC:
            typeof request.targetTempC === 'number'
              ? request.targetTempC
              : (selectedPresetTemp ?? baseStatus.targetTempC),
          selectedPresetSlot,
          presetsC,
          activeCoolingEnabled:
            typeof request.activeCoolingEnabled === 'boolean'
              ? request.activeCoolingEnabled
              : baseStatus.activeCoolingEnabled,
          heaterEnabled:
            typeof request.heaterEnabled === 'boolean'
              ? request.heaterEnabled
              : baseStatus.heaterEnabled,
          heaterOutputPercent: request.heaterEnabled === false ? 0 : baseStatus.heaterOutputPercent,
          fanDisplayState:
            request.activeCoolingEnabled === false ? 'OFF' : baseStatus.fanDisplayState,
        },
      },
    }
  }
  if (request.type === 'wifi_config') {
    return {
      type: 'response',
      requestId,
      ok: true,
      result: {
        wifi: {
          network: {
            ...network,
            state: request.op === 'clear' ? 'disabled' : 'saving',
            ssid: request.op === 'clear' ? null : request.ssid,
            wifiPasswordLength: request.op === 'clear' ? 0 : String(request.password ?? '').length,
            configurationGeneration: 1,
            transitionSequence: 1,
          },
        },
      },
    }
  }
  if (request.type === 'calibration_job') {
    return {
      type: 'response',
      requestId,
      ok: true,
      result: {
        calibration_job: {
          kind: request.kind,
          status: request.op === 'cancel' ? 'canceled' : 'running',
          progressPercent: 0,
          samplesCollected: 0,
          nextRequestMv: request.kind === 'vin_adc_auto' ? 11000 : 20000,
          message: null,
        },
      },
    }
  }
  if (request.type === 'calibration_config') {
    return {
      type: 'response',
      requestId,
      ok: true,
      result: {
        calibration: {
          rtdAdc: {
            samples: [
              request.channel === 'rtd_adc'
                ? {
                    observedMv: 997,
                    expectedMv: 970,
                    referenceTempC: request.referenceTempC,
                    targetAdcMv: request.targetAdcMv,
                  }
                : null,
              null,
              null,
              null,
              null,
              null,
              null,
              null,
            ],
            fittedFit: {
              gain: 1,
              offsetMv: -27,
              sampleCount: request.channel === 'rtd_adc' ? 1 : 0,
            },
            slots: {
              a: { gain: 1, offsetMv: 0 },
              b: { gain: 1, offsetMv: 0 },
            },
            activeSlot: 'a',
          },
          vinAdc: {
            samples: [null, null, null, null, null, null, null, null],
            fittedFit: { gain: 1, offsetMv: 0, sampleCount: 0 },
            slots: {
              a: { gain: 1, offsetMv: 0 },
              b: { gain: 1, offsetMv: 0 },
            },
            activeSlot: 'a',
          },
        },
      },
    }
  }
  return {
    type: 'response',
    requestId,
    ok: false,
    error: { code: 'unknown_op', message: 'unknown request', retryable: false },
  }
}

const identity = {
  deviceId: 'flux-purr-s3-001',
  firmwareVersion: '0.1.0',
  buildId: 'build-1',
  gitSha: 'abc',
  board: 'esp32-s3',
  apiVersion: '2026-05-29',
  protocolVersion: 'flux-purr.usb.v1',
  hostname: 'flux-purr-s3-001',
  capabilities: ['identity', 'status', 'network', 'usb_jsonl', 'monitor'],
}

const network = {
  state: 'connected',
  ssid: 'FluxPurr-Lab',
  ip: null,
  gateway: null,
  dns: [],
  wifiRssi: -48,
  lastError: null,
  configurationGeneration: 3,
  transitionSequence: 26,
  failureCode: null,
}

const baseStatus = {
  _firmwarePayloadPadding: 'x'.repeat(3_000),
  mode: 'sampling',
  uptimeSeconds: 3661,
  currentTempC: 181.5,
  targetTempC: 220,
  selectedPresetSlot: 7,
  presetsC: [50, 100, 120, null, 180, 200, 210, 220, 250, 300],
  heaterEnabled: true,
  heaterOutputPercent: 18,
  activeCoolingEnabled: true,
  fanDisplayState: 'AUTO',
  fanEnabled: true,
  fanPwmPermille: 500,
  voltageMv: 20000,
  currentMa: 820,
  boardTempCenti: 3720,
  pdRequestMv: 20000,
  pdContractMv: 20000,
  pdState: 'ready',
  frontpanelKey: null,
  network,
}
