import type { ESPLoader, Transport as EspTransport } from 'esptool-js'
import SparkMD5 from 'spark-md5'

import migrations from '../../../../docs/specs/web-firmware-install-recovery/contracts/migrations.json'
import type { FirmwareOperation, ValidatedFirmwareBundle } from './types'

export const ESP_GET_SECURITY_INFO = 0x14
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

export interface BrowserLayoutPreflight {
  sourcePartitionTableSha256: string | null
  configCopy: { sourceAddress: number; targetAddress: number; bytes: Uint8Array } | null
}

export async function connectBrowserLoader(): Promise<ESPLoader> {
  if (!window.isSecureContext || !('serial' in navigator)) {
    throw new Error('Browser USB requires desktop Chrome or Edge on HTTPS or localhost.')
  }
  const { ESPLoader, Transport } = await import('esptool-js')
  const port = await (
    navigator as Navigator & { serial: { requestPort(): Promise<BrowserSerialPort> } }
  ).serial.requestPort()
  const transport = new Transport(port, false)
  const loader = new ESPLoader({
    transport,
    baudrate: 115_200,
    debugLogging: false,
    terminal: { clean() {}, write() {}, writeLine() {} },
  })
  await loader.main('default_reset')
  return loader
}

export async function getEsp32S3SecurityInfo(loader: LoaderPort): Promise<BrowserSecurityInfo> {
  const [, payload] = await loader.command(ESP_GET_SECURITY_INFO, new Uint8Array(), 0, true, 3_000)
  return parseEsp32S3SecurityInfo(payload)
}

export function parseEsp32S3SecurityInfo(payload: Uint8Array): BrowserSecurityInfo {
  if (payload.byteLength !== 20) {
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

export async function preflightBrowserTarget(loader: LoaderPort) {
  if (loader.chip.CHIP_NAME !== 'ESP32-S3') {
    throw new Error('Only ESP32-S3 targets are supported.')
  }
  const flashSize = await loader.detectFlashSize()
  if (flashSize !== '4MB') {
    throw new Error('Target Flash must be exactly 4 MiB.')
  }
  const security = await getEsp32S3SecurityInfo(loader)
  if (
    !security.responseKnown ||
    security.secureBootEnabled ||
    security.flashEncryptionEnabled ||
    security.secureDownloadModeEnabled
  ) {
    throw new Error('Target security state blocks browser firmware installation.')
  }
  return security
}

export async function writeBrowserBundle(
  loader: LoaderPort,
  bundle: ValidatedFirmwareBundle,
  operation: FirmwareOperation,
  reportProgress?: (fileIndex: number, written: number, total: number) => void
) {
  await preflightBrowserTarget(loader)
  const layout = await preflightBrowserLayout(loader, bundle, operation)
  if (operation === 'install_recovery') await loader.eraseFlash()
  const fileArray = bundle.manifest.segments.map((segment) => {
    const data = bundle.images.get(segment.path)
    if (!data) throw new Error(`${segment.kind} image is missing from the validated bundle.`)
    return { data, address: segment.address }
  })
  await loader.writeFlash({
    fileArray,
    flashMode: 'dio',
    flashFreq: '40m',
    flashSize: '4MB',
    eraseAll: false,
    compress: true,
    reportProgress,
    calculateMD5Hash: (image) => {
      const copy = Uint8Array.from(image)
      return SparkMD5.ArrayBuffer.hash(copy.buffer as ArrayBuffer)
    },
  })
  for (const segment of bundle.manifest.segments) {
    const actual = await loader.flashMd5sum(segment.address, segment.length)
    if (actual.toLowerCase() !== segment.md5) throw new Error(`${segment.kind} ROM MD5 differs.`)
  }
  if (layout.configCopy) {
    await writeConfigCopy(loader, layout.configCopy)
    const restored = await loader.readFlash(
      layout.configCopy.targetAddress,
      layout.configCopy.bytes.byteLength
    )
    if (!equalBytes(restored, layout.configCopy.bytes)) {
      throw new Error('Restored flux_cfg differs from the staged source bytes.')
    }
  }
  await loader.after('hard_reset')
}

export async function preflightBrowserLayout(
  loader: LoaderPort,
  bundle: ValidatedFirmwareBundle,
  operation: FirmwareOperation
): Promise<BrowserLayoutPreflight> {
  if (operation === 'install_recovery') {
    return { sourcePartitionTableSha256: null, configCopy: null }
  }
  const partitionTable = await loader.readFlash(0x8000, 0x1000)
  if (partitionTable.byteLength !== 0x1000) {
    throw new Error('Current partition table could not be read exactly.')
  }
  const sourceHash = `sha256:${await sha256Hex(partitionTable)}`
  if (sourceHash === bundle.manifest.layout.partitionTableSha256) {
    return { sourcePartitionTableSha256: sourceHash, configCopy: null }
  }
  const migration = migrations.migrations.find(
    (candidate) =>
      candidate.sourcePartitionTableSha256 === sourceHash &&
      bundle.manifest.migrations.includes(candidate.id)
  )
  if (!migration || migration.copies.length !== 1) {
    throw new Error('Current partition-table hash has no declared supported migration.')
  }
  const copy = migration.copies[0]
  const bytes = await loader.readFlash(copy.sourceAddress, copy.length)
  if (bytes.byteLength !== copy.length) {
    throw new Error('Source flux_cfg could not be staged exactly.')
  }
  return {
    sourcePartitionTableSha256: sourceHash,
    configCopy: { sourceAddress: copy.sourceAddress, targetAddress: copy.targetAddress, bytes },
  }
}

async function writeConfigCopy(
  loader: LoaderPort,
  copy: NonNullable<BrowserLayoutPreflight['configCopy']>
) {
  await loader.writeFlash({
    fileArray: [{ data: copy.bytes, address: copy.targetAddress }],
    flashMode: 'dio',
    flashFreq: '40m',
    flashSize: '4MB',
    eraseAll: false,
    compress: true,
    calculateMD5Hash: (image) =>
      SparkMD5.ArrayBuffer.hash(Uint8Array.from(image).buffer as ArrayBuffer),
  })
}

export async function verifyBrowserRuntime(
  loader: ESPLoader,
  bundle: ValidatedFirmwareBundle,
  timeoutMs = 12_000
) {
  await loader.transport.disconnect()
  const port = loader.transport.device
  await new Promise((resolve) => window.setTimeout(resolve, 1_000))
  await port.open({ baudRate: 115_200 })
  const writer = port.writable?.getWriter()
  const reader = port.readable?.getReader()
  if (!writer || !reader) throw new Error('Runtime serial streams are unavailable after reset.')
  const encoder = new TextEncoder()
  const decoder = new TextDecoder()
  let buffer = ''
  let identity: Record<string, unknown> | null = null
  let installStatus: Record<string, unknown> | null = null
  const deadline = Date.now() + timeoutMs
  try {
    await writer.write(
      encoder.encode(
        '{"type":"request","requestId":"firmware-identity","op":"get_identity"}\n' +
          '{"type":"request","requestId":"firmware-install-status","op":"get_install_status"}\n'
      )
    )
    while (Date.now() < deadline && (!identity || !installStatus)) {
      const remaining = Math.max(1, deadline - Date.now())
      const result = await Promise.race([
        reader.read(),
        new Promise<never>((_, reject) =>
          window.setTimeout(() => reject(new Error('Runtime verification timed out.')), remaining)
        ),
      ])
      if (result.done) break
      buffer += decoder.decode(result.value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() ?? ''
      for (const line of lines) {
        if (!line.trim()) continue
        const frame = JSON.parse(line) as Record<string, unknown>
        const resultValue = frame.result as Record<string, unknown> | undefined
        if (frame.requestId === 'firmware-identity') {
          identity = (resultValue?.identity ?? resultValue) as Record<string, unknown>
        }
        if (frame.requestId === 'firmware-install-status') {
          installStatus = (resultValue?.install_status ??
            resultValue?.installStatus ??
            resultValue) as Record<string, unknown>
        }
      }
    }
  } finally {
    reader.releaseLock()
    writer.releaseLock()
    await port.close().catch(() => undefined)
  }
  if (!identity || !installStatus)
    throw new Error('Runtime identity or install status was not returned.')
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

function popcount(value: number) {
  let count = 0
  for (let bits = value; bits > 0; bits >>= 1) count += bits & 1
  return count
}

async function sha256Hex(bytes: Uint8Array) {
  const digest = await crypto.subtle.digest('SHA-256', Uint8Array.from(bytes))
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('')
}

function equalBytes(left: Uint8Array, right: Uint8Array) {
  return left.byteLength === right.byteLength && left.every((byte, index) => byte === right[index])
}
