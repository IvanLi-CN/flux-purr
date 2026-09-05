import type { ESPLoader, Transport as EspTransport } from 'esptool-js'
import SparkMD5 from 'spark-md5'

import {
  type BrowserSerial,
  FLUX_PURR_USB_SERIAL_REQUEST_OPTIONS,
} from '../control-plane-demo/web-serial'
import type { FirmwareOperation, ValidatedFirmwareBundle } from './types'

export const ESP_GET_SECURITY_INFO = 0x14
const ESP_SECURITY_INFO_BYTES = 20
const ESP_ROM_SECURITY_INFO_TRAILER_BYTES = 4
const RUNTIME_READY_BOOT_STAGE = 'boot_stage=runtime_ready'
const MAX_RUNTIME_IDENTITY_REQUEST_ATTEMPTS = 18
// Native USB Serial/JTAG needs a second, application-directed reset after the
// generic ROM reset. Keep this sequence aligned with the established
// ESP32-S3 Web Serial implementation rather than treating UsbJtagSerialReset
// as the post-flash application reset.
const ESP32_S3_USB_JTAG_APP_RESET_SEQUENCE = 'D0|R0|W50|D1|R0|W50|D0|R1|W50|D0|R0|W250'
type BrowserSerialPort = ConstructorParameters<typeof EspTransport>[0]

export interface BrowserSecurityInfo {
  secureBootEnabled: boolean
  flashEncryptionEnabled: boolean
  secureDownloadModeEnabled: boolean
  responseKnown: boolean
}

type LoaderPort = Pick<
  ESPLoader,
  | 'command'
  | 'eraseFlash'
  | 'writeFlash'
  | 'readFlash'
  | 'flashMd5sum'
  | 'after'
  | 'detectFlashSize'
  | 'chip'
>

// esptool-js uses a 200 ms timeout when it sends ESP_MEM_END after uploading
// the flasher stub. ESP32-S3 native USB Serial/JTAG needs the longer handoff
// used by the established Web Serial implementation, otherwise the target can
// remain in the download path after a successful ROM write.
type Esp32S3StubLoader = {
  ESP_MEM_END: number
  _appendArray(left: Uint8Array, right: Uint8Array): Uint8Array
  _intToByteArray(value: number): Uint8Array
  checkCommand(
    opDescription: string,
    op: number,
    data: Uint8Array,
    checksum?: number,
    responseDataLength?: number,
    timeout?: number
  ): Promise<unknown>
  memFinish(entrypoint: number): Promise<void>
}

interface PreparedBrowserLoader {
  operation: FirmwareOperation
  layout: BrowserLayoutPreflight
}

const preparedBrowserLoaders = new WeakMap<object, PreparedBrowserLoader>()

export interface BrowserLayoutPreflight {
  sourcePartitionTableSha256: string | null
}

export type BrowserRuntimeVerificationStage =
  | 'disconnecting_rom'
  | 'waiting_for_runtime'
  | 'opening_runtime'
  | 'requesting_identity'
  | 'reading_runtime'
  | 'closing_runtime'

export interface BrowserRuntimeVerificationEvent {
  stage: BrowserRuntimeVerificationStage
  attempt?: number
}

export interface BrowserRuntimeVerificationOptions {
  timeoutMs?: number
  boundaryTimeoutMs?: number
  reconnectDelayMs?: number
  reconnectRetryMs?: number
  requestRetryMs?: number
  romTransportAlreadyDisconnected?: boolean
  reportStage?: (event: BrowserRuntimeVerificationEvent) => void
}

export type BrowserWriteStage =
  | 'erase_started'
  | 'erase_completed'
  | 'write_started'
  | 'write_progress'
  | 'rom_md5_started'
  | 'rom_md5_progress'
  | 'reset_started'
  | 'reset_completed'

export interface BrowserWriteProgressEvent {
  stage: BrowserWriteStage
  segmentIndex?: number
  written?: number
  total?: number
  totalBytes?: number
  completedSegments?: number
  totalSegments?: number
}

export interface BrowserWriteOptions {
  reportProgress?: (fileIndex: number, written: number, total: number) => void
  reportStage?: (event: BrowserWriteProgressEvent) => void
}

export async function connectBrowserLoader(port?: BrowserSerialPort): Promise<ESPLoader> {
  if (!window.isSecureContext || !('serial' in navigator)) {
    throw new Error('Browser USB requires desktop Chrome or Edge on HTTPS or localhost.')
  }
  const { ESPLoader, Transport } = await import('esptool-js')
  const serial = (navigator as Navigator & { serial?: BrowserSerial }).serial
  if (!serial) {
    throw new Error('Browser USB requires desktop Chrome or Edge on HTTPS or localhost.')
  }
  const selectedPort =
    port ?? ((await serial.requestPort(FLUX_PURR_USB_SERIAL_REQUEST_OPTIONS)) as BrowserSerialPort)
  const transport = new Transport(selectedPort, false)
  const loader = new ESPLoader({
    transport,
    baudrate: 115_200,
    debugLogging: false,
    terminal: { clean() {}, write() {}, writeLine() {} },
  })
  try {
    // Security must be read from the ROM bootloader. `main()` uploads the
    // flasher stub before returning, and the stub does not preserve the ROM
    // GET_SECURITY_INFO response contract.
    await loader.detectChip('usb_reset')
    return loader
  } catch (error) {
    await transport.disconnect().catch(() => undefined)
    throw error
  }
}

export async function disconnectBrowserLoader(
  loader: Pick<ESPLoader, 'transport'> | null | undefined
) {
  if (loader) preparedBrowserLoaders.delete(loader)
  await loader?.transport.disconnect().catch(() => undefined)
}

export async function getEsp32S3SecurityInfo(loader: LoaderPort): Promise<BrowserSecurityInfo> {
  const [, payload] = await loader.command(ESP_GET_SECURITY_INFO, new Uint8Array(), 0, true, 3_000)
  return parseEsp32S3SecurityInfo(normalizeEsp32S3SecurityInfoPayload(payload))
}

export function parseEsp32S3SecurityInfo(payload: Uint8Array): BrowserSecurityInfo {
  if (payload.byteLength !== ESP_SECURITY_INFO_BYTES) {
    return {
      secureBootEnabled: false,
      flashEncryptionEnabled: false,
      secureDownloadModeEnabled: false,
      responseKnown: false,
    }
  }
  const flags = new DataView(payload.buffer, payload.byteOffset, payload.byteLength).getUint32(
    0,
    true
  )
  const knownFlagMask = 0x7ff
  if ((flags & ~knownFlagMask) !== 0) {
    return {
      secureBootEnabled: false,
      flashEncryptionEnabled: false,
      secureDownloadModeEnabled: false,
      responseKnown: false,
    }
  }
  const flashCryptCount = payload[4]
  return {
    secureBootEnabled: (flags & 0x1) !== 0,
    flashEncryptionEnabled: popcount(flashCryptCount) % 2 === 1,
    secureDownloadModeEnabled: (flags & 0x4) !== 0,
    responseKnown: true,
  }
}

function normalizeEsp32S3SecurityInfoPayload(payload: Uint8Array) {
  // `espflash` removes the four-byte ROM trailer, then parses the leading
  // SecurityInfo record even when a newer ROM appends more response data.
  // Only the first 20 bytes are defined by GET_SECURITY_INFO. Accept either
  // that exact record or a complete ROM response with its four-byte trailer;
  // partial 21-23 byte responses remain fail-closed.
  if (payload.byteLength >= ESP_SECURITY_INFO_BYTES + ESP_ROM_SECURITY_INFO_TRAILER_BYTES) {
    return payload.slice(0, ESP_SECURITY_INFO_BYTES)
  }
  return payload
}

export async function preflightBrowserTarget(loader: LoaderPort) {
  if (loader.chip.CHIP_NAME !== 'ESP32-S3') {
    throw new Error('Only ESP32-S3 targets are supported.')
  }
  const flashSize = await loader.detectFlashSize()
  if (flashSize !== '4MB') {
    throw new Error('Target Flash must be exactly 4 MiB.')
  }
  const features = await loader.chip.getChipFeatures(loader as ESPLoader)
  if (
    !features.some((feature) => feature.startsWith('Embedded Flash 4MB')) ||
    !features.some((feature) => feature.startsWith('Embedded PSRAM 2MB'))
  ) {
    throw new Error('Target package must expose embedded 4 MiB Flash and 2 MiB PSRAM.')
  }
  const security = await getEsp32S3SecurityInfo(loader)
  if (
    !security.responseKnown ||
    security.secureBootEnabled ||
    security.flashEncryptionEnabled ||
    security.secureDownloadModeEnabled
  ) {
    throw new Error(browserSecurityBlockMessage(security))
  }
  return security
}

export async function preflightBrowserLoader(
  loader: LoaderPort & Pick<ESPLoader, 'transport' | 'runStub'>,
  bundle: ValidatedFirmwareBundle,
  operation: FirmwareOperation
) {
  try {
    await preflightBrowserTarget(loader)
    patchEsp32S3UsbJtagStubStart(loader as unknown as Esp32S3StubLoader)
    await loader.runStub()
    const layout = await preflightBrowserLayout(loader, bundle, operation)
    preparedBrowserLoaders.set(loader, { operation, layout })
    return layout
  } catch (error) {
    await disconnectBrowserLoader(loader)
    throw error
  }
}

function patchEsp32S3UsbJtagStubStart(loader: Esp32S3StubLoader) {
  loader.memFinish = async (entrypoint: number) => {
    const isEntry = entrypoint === 0 ? 1 : 0
    const packet = loader._appendArray(
      loader._intToByteArray(isEntry),
      loader._intToByteArray(entrypoint)
    )
    await loader.checkCommand(
      'leave RAM download mode',
      loader.ESP_MEM_END,
      packet,
      undefined,
      undefined,
      2_000
    )
  }
}

export function browserSecurityBlockMessage(security: BrowserSecurityInfo) {
  const restrictions: string[] = []
  if (!security.responseKnown) restrictions.push('ROM 安全响应未知')
  if (security.secureBootEnabled) restrictions.push('Secure Boot 已启用')
  if (security.flashEncryptionEnabled) restrictions.push('Flash Encryption 已启用')
  if (security.secureDownloadModeEnabled) restrictions.push('Secure Download Mode 已启用')
  return `芯片安全状态阻止浏览器烧录：${restrictions.join('、')}。`
}

export async function writeBrowserBundle(
  loader: LoaderPort,
  bundle: ValidatedFirmwareBundle,
  operation: FirmwareOperation,
  optionsOrProgress?: BrowserWriteOptions | BrowserWriteOptions['reportProgress']
) {
  const options = normalizeBrowserWriteOptions(optionsOrProgress)
  const prepared = preparedBrowserLoaders.get(loader)
  if (!prepared || prepared.operation !== operation) {
    throw new Error('Browser ROM preflight must complete for this operation before writing.')
  }
  try {
    if (operation === 'install_recovery') {
      options.reportStage?.({ stage: 'erase_started' })
      await loader.eraseFlash()
      options.reportStage?.({ stage: 'erase_completed' })
    }
    const fileArray = bundle.manifest.segments.map((segment) => {
      const data = bundle.images.get(segment.path)
      if (!data) throw new Error(`${segment.kind} image is missing from the validated bundle.`)
      return { data, address: segment.address }
    })
    options.reportStage?.({
      stage: 'write_started',
      totalBytes: fileArray.reduce((total, file) => total + file.data.byteLength, 0),
    })
    await loader.writeFlash({
      fileArray,
      flashMode: 'dio',
      flashFreq: '40m',
      flashSize: '4MB',
      eraseAll: false,
      compress: true,
      reportProgress: (fileIndex, written, total) => {
        options.reportProgress?.(fileIndex, written, total)
        options.reportStage?.({
          stage: 'write_progress',
          segmentIndex: fileIndex,
          written,
          total,
        })
      },
      calculateMD5Hash: (image) => {
        const copy = Uint8Array.from(image)
        return SparkMD5.ArrayBuffer.hash(copy.buffer as ArrayBuffer)
      },
    })
    options.reportStage?.({
      stage: 'rom_md5_started',
      totalSegments: bundle.manifest.segments.length,
    })
    for (const [segmentIndex, segment] of bundle.manifest.segments.entries()) {
      const actual = await loader.flashMd5sum(segment.address, segment.length)
      if (actual.toLowerCase() !== segment.md5) throw new Error(`${segment.kind} ROM MD5 differs.`)
      options.reportStage?.({
        stage: 'rom_md5_progress',
        segmentIndex,
        completedSegments: segmentIndex + 1,
        totalSegments: bundle.manifest.segments.length,
      })
    }
    options.reportStage?.({ stage: 'reset_started' })
    await loader.after('hard_reset')
    await loader.after('custom_reset', undefined, ESP32_S3_USB_JTAG_APP_RESET_SEQUENCE)
    options.reportStage?.({ stage: 'reset_completed' })
  } finally {
    preparedBrowserLoaders.delete(loader)
  }
}

function normalizeBrowserWriteOptions(
  input: BrowserWriteOptions | BrowserWriteOptions['reportProgress'] | undefined
): BrowserWriteOptions {
  return typeof input === 'function' ? { reportProgress: input } : (input ?? {})
}

export async function preflightBrowserLayout(
  loader: LoaderPort,
  bundle: ValidatedFirmwareBundle,
  operation: FirmwareOperation
): Promise<BrowserLayoutPreflight> {
  if (operation === 'install_recovery') {
    return { sourcePartitionTableSha256: null }
  }
  const partitionTable = await loader.readFlash(0x8000, 0x1000)
  if (partitionTable.byteLength !== 0x1000) {
    throw new Error('Current partition table could not be read exactly.')
  }
  const sourceHash = `sha256:${await sha256Hex(partitionTable)}`
  if (sourceHash === bundle.manifest.layout.partitionTableSha256) {
    return { sourcePartitionTableSha256: sourceHash }
  }
  throw new Error('Current partition-table hash does not match the bundle layout.')
}

export async function verifyBrowserRuntime(
  loader: ESPLoader,
  bundle: ValidatedFirmwareBundle,
  optionsOrTimeout: BrowserRuntimeVerificationOptions | number = {}
) {
  const options = normalizeRuntimeVerificationOptions(optionsOrTimeout)
  const deadline = Date.now() + options.timeoutMs
  const romPort = loader.transport.device
  options.reportStage?.({ stage: 'disconnecting_rom' })
  if (!options.romTransportAlreadyDisconnected) {
    await runRuntimeBoundary(
      loader.transport.disconnect(),
      deadline,
      options.boundaryTimeoutMs,
      'disconnecting ROM transport'
    )
  }
  options.reportStage?.({ stage: 'waiting_for_runtime' })
  await delayWithinRuntimeDeadline(options.reconnectDelayMs, deadline, 'waiting for runtime USB')
  const port = await refreshGrantedBrowserRuntimePort(romPort, deadline, options.reconnectRetryMs)
  await openRuntimePort(port, deadline, options)

  const writer = port.writable?.getWriter()
  const reader = port.readable?.getReader()
  if (!writer || !reader) {
    await closeRuntimePort(port, options).catch(() => undefined)
    throw new Error('Runtime serial streams are unavailable after reset.')
  }
  const encoder = new TextEncoder()
  const decoder = new TextDecoder()
  let buffer = ''
  let identity: Record<string, unknown> | null = null
  let installStatus: Record<string, unknown> | null = null
  let pendingRead: Promise<ReadableStreamReadResult<Uint8Array>> | null = null
  let requestAttempt = 0
  let identityRequestAttempts = 0
  let installStatusRequestAttempts = 0
  let nextIdentityRequestAt = Date.now()
  let nextInstallStatusRequestAt = Number.POSITIVE_INFINITY
  let installStatusWaitingForRuntime = false
  let verificationError: unknown = null
  try {
    while (Date.now() < deadline && (!identity || !installStatus)) {
      const now = Date.now()
      if (
        !identity &&
        identityRequestAttempts < MAX_RUNTIME_IDENTITY_REQUEST_ATTEMPTS &&
        now >= nextIdentityRequestAt
      ) {
        requestAttempt += 1
        identityRequestAttempts += 1
        options.reportStage?.({ stage: 'requesting_identity', attempt: requestAttempt })
        await writeRuntimeRequest(
          writer,
          runtimeRequestBytes(encoder, 'firmware-identity', 'get_identity'),
          deadline,
          options.boundaryTimeoutMs
        )
        nextIdentityRequestAt = Date.now() + options.requestRetryMs
      } else if (
        identity &&
        !installStatus &&
        !installStatusWaitingForRuntime &&
        installStatusRequestAttempts < 2 &&
        now >= nextInstallStatusRequestAt
      ) {
        requestAttempt += 1
        installStatusRequestAttempts += 1
        options.reportStage?.({ stage: 'requesting_identity', attempt: requestAttempt })
        await writeRuntimeRequest(
          writer,
          runtimeRequestBytes(encoder, 'firmware-install-status', 'get_install_status'),
          deadline,
          options.boundaryTimeoutMs
        )
        nextInstallStatusRequestAt = Date.now() + options.requestRetryMs
      }
      options.reportStage?.({ stage: 'reading_runtime', attempt: requestAttempt })
      const currentRead: Promise<ReadableStreamReadResult<Uint8Array>> =
        pendingRead ?? reader.read()
      pendingRead = currentRead
      const remaining = remainingRuntimeTime(deadline, 'reading runtime responses')
      const nextRequestAt = Math.min(
        !identity && identityRequestAttempts < MAX_RUNTIME_IDENTITY_REQUEST_ATTEMPTS
          ? nextIdentityRequestAt
          : Number.POSITIVE_INFINITY,
        identity &&
          !installStatus &&
          !installStatusWaitingForRuntime &&
          installStatusRequestAttempts < 2
          ? nextInstallStatusRequestAt
          : Number.POSITIVE_INFINITY
      )
      const untilNextRequest = Number.isFinite(nextRequestAt)
        ? Math.max(1, nextRequestAt - Date.now())
        : remaining
      const result = await readOrDelay(currentRead, Math.min(remaining, untilNextRequest))
      if (!result) continue
      pendingRead = null
      if (result.done) break
      buffer += decoder.decode(result.value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() ?? ''
      for (const line of lines) {
        if (!line.trim()) continue
        if (line.trim() === RUNTIME_READY_BOOT_STAGE) {
          // The marker is a useful diagnostic, but it travels through the
          // best-effort boot log stream. Runtime verification must remain
          // correct when that line is absent or dropped.
          if (!identity && identityRequestAttempts < MAX_RUNTIME_IDENTITY_REQUEST_ATTEMPTS) {
            nextIdentityRequestAt = Date.now()
          }
          if (installStatusWaitingForRuntime && installStatusRequestAttempts < 2) {
            installStatusWaitingForRuntime = false
            nextInstallStatusRequestAt = Date.now()
          }
          continue
        }
        let frame: Record<string, unknown>
        try {
          frame = JSON.parse(line) as Record<string, unknown>
        } catch {
          // Boot diagnostics can share the runtime serial stream with JSONL.
          continue
        }
        const resultValue = frame.result as Record<string, unknown> | undefined
        if (frame.requestId === 'firmware-identity') {
          identity = (resultValue?.identity ?? resultValue) as Record<string, unknown>
          nextInstallStatusRequestAt = Date.now()
        }
        if (frame.requestId === 'firmware-install-status') {
          const error = frame.error as Record<string, unknown> | undefined
          const errorCode = error?.code ?? frame.code
          if (errorCode === 'startup_busy') {
            // The firmware received this request but has not started its control
            // plane. Wait for its boot marker instead of queueing duplicates.
            installStatusWaitingForRuntime = true
          } else {
            installStatus = (resultValue?.install_status ??
              resultValue?.installStatus ??
              resultValue) as Record<string, unknown>
          }
        }
      }
    }
    if (!identity || !installStatus) {
      throw runtimeVerificationTimeout('reading runtime identity and install status')
    }
  } catch (error) {
    verificationError = error
  } finally {
    if (pendingRead) {
      const cancelError = runtimeVerificationTimeout('cancelling runtime response read')
      const cancel = reader.cancel(cancelError)
      await settleRuntimeCleanup(cancel, options.boundaryTimeoutMs)
      await settleRuntimeCleanup(pendingRead, options.boundaryTimeoutMs)
    }
    if (writer.desiredSize === null) {
      await settleRuntimeCleanup(writer.abort(verificationError), options.boundaryTimeoutMs)
    }
    try {
      reader.releaseLock()
    } catch (error) {
      verificationError ??= runtimeStreamReleaseError('readable', error)
    }
    try {
      writer.releaseLock()
    } catch (error) {
      verificationError ??= runtimeStreamReleaseError('writable', error)
    }
    try {
      await closeRuntimePort(port, options)
    } catch (error) {
      verificationError ??= error
    }
  }
  if (verificationError) throw verificationError
  if (!identity || !installStatus) throw runtimeVerificationTimeout('reading runtime responses')
  if (
    identity.firmwareVersion !== bundle.manifest.identity.version ||
    identity.gitSha !== bundle.manifest.identity.sourceSha ||
    identity.buildId !== bundle.manifest.identity.buildId ||
    installStatus.layoutId !== bundle.manifest.layout.id ||
    installStatus.layoutVersion !== bundle.manifest.layout.version ||
    installStatus.partitionTableSha256 !== bundle.manifest.layout.partitionTableSha256
  ) {
    throw new Error('Runtime identity or layout does not match the installed bundle.')
  }
  return { identity, installStatus }
}

async function refreshGrantedBrowserRuntimePort(
  preferred: BrowserSerialPort,
  deadline: number,
  retryMs: number
): Promise<BrowserSerialPort> {
  if (typeof navigator === 'undefined') return preferred
  const serial = (navigator as Navigator & { serial?: BrowserSerial }).serial
  if (!serial?.getPorts) return preferred

  const preferredInfo = preferred.getInfo?.()
  for (;;) {
    const granted = await serial.getPorts()
    const sameObject = granted.find((port) => port === preferred)
    if (sameObject) return sameObject as BrowserSerialPort

    if (!preferredInfo) {
      throw new Error(
        'Browser cannot prove the selected Web USB target after reset. Re-open Web USB and choose the exact ESP32-S3 target again.'
      )
    }

    const matchingPorts = granted.filter((port) =>
      sameBrowserUsbInfo(port.getInfo?.(), preferredInfo)
    )
    if (matchingPorts.length === 1) return matchingPorts[0] as BrowserSerialPort
    if (matchingPorts.length > 1) {
      throw new Error(
        'Browser granted Web USB ports are ambiguous after reset. Re-open Web USB and choose the exact ESP32-S3 target again.'
      )
    }
    await delayWithinRuntimeDeadline(
      retryMs,
      deadline,
      'waiting for the selected runtime serial port'
    )
  }
}

function sameBrowserUsbInfo(
  left: { usbVendorId?: number; usbProductId?: number } | undefined,
  right: { usbVendorId?: number; usbProductId?: number } | undefined
) {
  return (
    left?.usbVendorId !== undefined &&
    left?.usbProductId !== undefined &&
    left.usbVendorId === right?.usbVendorId &&
    left.usbProductId === right?.usbProductId
  )
}

interface NormalizedBrowserRuntimeVerificationOptions {
  timeoutMs: number
  boundaryTimeoutMs: number
  reconnectDelayMs: number
  reconnectRetryMs: number
  requestRetryMs: number
  romTransportAlreadyDisconnected: boolean
  reportStage?: (event: BrowserRuntimeVerificationEvent) => void
}

function normalizeRuntimeVerificationOptions(
  input: BrowserRuntimeVerificationOptions | number
): NormalizedBrowserRuntimeVerificationOptions {
  const options = typeof input === 'number' ? { timeoutMs: input } : input
  return {
    timeoutMs: positiveDuration(options.timeoutMs, 45_000),
    boundaryTimeoutMs: positiveDuration(options.boundaryTimeoutMs, 2_500),
    reconnectDelayMs: nonNegativeDuration(options.reconnectDelayMs, 1_000),
    reconnectRetryMs: positiveDuration(options.reconnectRetryMs, 400),
    requestRetryMs: positiveDuration(options.requestRetryMs, 2_500),
    romTransportAlreadyDisconnected: options.romTransportAlreadyDisconnected ?? false,
    reportStage: options.reportStage,
  }
}

function runtimeRequestBytes(encoder: TextEncoder, requestId: string, op: string) {
  return encoder.encode(`${JSON.stringify({ type: 'request', requestId, op })}\n`)
}

async function openRuntimePort(
  port: BrowserSerialPort,
  deadline: number,
  options: NormalizedBrowserRuntimeVerificationOptions
) {
  for (let attempt = 1; ; attempt += 1) {
    options.reportStage?.({ stage: 'opening_runtime', attempt })
    const openOperation = port.open({ baudRate: 115_200 })
    try {
      await runRuntimeBoundary(
        openOperation,
        deadline,
        remainingRuntimeTime(deadline, 'opening runtime serial port'),
        'opening runtime serial port'
      )
      return
    } catch (error) {
      if (isRuntimeBoundaryTimeout(error)) {
        void openOperation.then(() => port.close()).catch(() => undefined)
        throw error
      }
      if (!isRetryableRuntimeOpenError(error)) throw error
      await delayWithinRuntimeDeadline(
        options.reconnectRetryMs,
        deadline,
        'waiting for the selected runtime serial port to return'
      )
    }
  }
}

async function writeRuntimeRequest(
  writer: WritableStreamDefaultWriter<Uint8Array>,
  bytes: Uint8Array,
  deadline: number,
  boundaryTimeoutMs: number
) {
  const operation = writer.write(bytes)
  try {
    await runRuntimeBoundary(operation, deadline, boundaryTimeoutMs, 'requesting runtime identity')
  } catch (error) {
    await settleRuntimeCleanup(writer.abort(error), boundaryTimeoutMs)
    await settleRuntimeCleanup(operation, boundaryTimeoutMs)
    throw error
  }
}

async function closeRuntimePort(
  port: BrowserSerialPort,
  options: NormalizedBrowserRuntimeVerificationOptions
) {
  options.reportStage?.({ stage: 'closing_runtime' })
  await runRuntimeBoundary(
    port.close(),
    Date.now() + options.boundaryTimeoutMs,
    options.boundaryTimeoutMs,
    'closing runtime serial port'
  )
}

async function runRuntimeBoundary<T>(
  operation: Promise<T>,
  deadline: number,
  boundaryTimeoutMs: number,
  stage: string
) {
  const timeoutMs = Math.min(boundaryTimeoutMs, remainingRuntimeTime(deadline, stage))
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_, reject) => {
        timer = globalThis.setTimeout(() => reject(runtimeVerificationTimeout(stage)), timeoutMs)
      }),
    ])
  } finally {
    if (timer !== undefined) globalThis.clearTimeout(timer)
  }
}

async function delayWithinRuntimeDeadline(timeoutMs: number, deadline: number, stage: string) {
  if (timeoutMs <= 0) return
  const duration = Math.min(timeoutMs, remainingRuntimeTime(deadline, stage))
  await new Promise<void>((resolve) => globalThis.setTimeout(resolve, duration))
  remainingRuntimeTime(deadline, stage)
}

async function readOrDelay<T>(operation: Promise<T>, timeoutMs: number): Promise<T | null> {
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    return await Promise.race([
      operation,
      new Promise<null>((resolve) => {
        timer = globalThis.setTimeout(() => resolve(null), timeoutMs)
      }),
    ])
  } finally {
    if (timer !== undefined) globalThis.clearTimeout(timer)
  }
}

async function settleRuntimeCleanup(operation: Promise<unknown>, timeoutMs: number) {
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    await Promise.race([
      operation.catch(() => undefined),
      new Promise<void>((resolve) => {
        timer = globalThis.setTimeout(resolve, timeoutMs)
      }),
    ])
  } finally {
    if (timer !== undefined) globalThis.clearTimeout(timer)
  }
}

function remainingRuntimeTime(deadline: number, stage: string) {
  const remaining = deadline - Date.now()
  if (remaining <= 0) throw runtimeVerificationTimeout(stage)
  return remaining
}

function runtimeVerificationTimeout(stage: string) {
  const error = new Error(`Runtime verification timed out while ${stage}.`)
  error.name = 'RuntimeVerificationTimeoutError'
  return error
}

function runtimeStreamReleaseError(stream: 'readable' | 'writable', cause: unknown) {
  const detail = cause instanceof Error ? ` ${cause.message}` : ''
  return new Error(`Runtime ${stream} stream remained locked during cleanup.${detail}`)
}

function isRuntimeBoundaryTimeout(error: unknown) {
  return error instanceof Error && error.name === 'RuntimeVerificationTimeoutError'
}

function isRetryableRuntimeOpenError(error: unknown) {
  if (error instanceof DOMException) {
    return error.name === 'NetworkError' || error.name === 'NotFoundError'
  }
  return (
    error instanceof Error &&
    /disconnected|device unavailable|device has been lost|failed to open/i.test(error.message)
  )
}

function positiveDuration(value: number | undefined, fallback: number) {
  return Number.isFinite(value) && Number(value) > 0 ? Number(value) : fallback
}

function nonNegativeDuration(value: number | undefined, fallback: number) {
  return Number.isFinite(value) && Number(value) >= 0 ? Number(value) : fallback
}

function popcount(value: number) {
  let count = 0
  for (let bits = value; bits > 0; bits >>= 1) count += bits & 1
  return count
}

async function sha256Hex(bytes: Uint8Array) {
  const digest = await crypto.subtle.digest('SHA-256', Uint8Array.from(bytes))
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('')
}
