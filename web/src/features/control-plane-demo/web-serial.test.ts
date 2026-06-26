import { describe, expect, it } from 'vitest'
import type { BrowserSerial, BrowserSerialPort } from './web-serial'
import {
  isDirectWebSerialDevice,
  WebSerialControlPlaneClient,
  webSerialProbeToDeviceTarget,
} from './web-serial'

describe('web serial control-plane client', () => {
  it('probes firmware over USB JSONL and maps the direct device target', async () => {
    const fake = new FakeSerial()
    const client = new WebSerialControlPlaneClient({ serial: fake })

    const probe = await client.connect()
    const target = webSerialProbeToDeviceTarget(probe)

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
    })
    expect(target.capabilities).toContain('usb_jsonl')
    expect(isDirectWebSerialDevice(target)).toBe(true)

    await client.disconnect()
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
})

class FakeSerial implements BrowserSerial {
  readonly requests: Array<Record<string, unknown>> = []
  private readonly port = new FakeSerialPort(this.requests)

  requestPort(): Promise<BrowserSerialPort> {
    return Promise.resolve(this.port)
  }
}

class FakeSerialPort implements BrowserSerialPort {
  readonly readable: ReadableStream<Uint8Array>
  readonly writable: WritableStream<Uint8Array>
  private controller: ReadableStreamDefaultController<Uint8Array> | null = null
  private readonly decoder = new TextDecoder()
  private readonly encoder = new TextEncoder()
  private readonly requests: Array<Record<string, unknown>>
  private writeBuffer = ''

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
      this.controller?.enqueue(this.encoder.encode(`${JSON.stringify(responseFor(request))}\n`))
      newlineIndex = this.writeBuffer.indexOf('\n')
    }
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
          active: {
            rtdAdc: [null, null, null, null, null, null, null, null],
            vinAdc: [null, null, null, null, null, null, null, null],
          },
          draft: {
            rtdAdc: [
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
            vinAdc: [null, null, null, null, null, null, null, null],
          },
          activeFit: {
            rtdAdc: { gain: 1, offsetMv: 0, customSampleCount: 0, defaultSampleCount: 2 },
            vinAdc: { gain: 1, offsetMv: 0, customSampleCount: 0, defaultSampleCount: 2 },
          },
          draftFit: {
            rtdAdc: { gain: 1, offsetMv: 0, customSampleCount: 1, defaultSampleCount: 2 },
            vinAdc: { gain: 1, offsetMv: 0, customSampleCount: 0, defaultSampleCount: 2 },
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
  state: 'idle',
  ssid: null,
  ip: null,
  gateway: null,
  dns: [],
  wifiRssi: null,
  lastError: null,
}

const baseStatus = {
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
