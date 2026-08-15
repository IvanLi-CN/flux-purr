import Ajv2020 from 'ajv/dist/2020'
import { unzipSync } from 'fflate'
import SparkMD5 from 'spark-md5'

import schema from '../../../../docs/specs/web-firmware-install-recovery/contracts/firmware-bundle.schema.json'
import migrations from '../../../../docs/specs/web-firmware-install-recovery/contracts/migrations.json'
import type { FirmwareManifest, ValidatedFirmwareBundle } from './types'

const MAX_BUNDLE_BYTES = 8 * 1024 * 1024
const REQUIRED_PATHS = [
  'manifest.json',
  'images/bootloader.bin',
  'images/partition-table.bin',
  'images/factory-app.bin',
] as const
const ajv = new Ajv2020({ allErrors: true, strict: true })
const validateManifest = ajv.compile<FirmwareManifest>(schema)

export class FirmwareBundleError extends Error {
  readonly code: string

  constructor(code: string, message: string) {
    super(message)
    this.name = 'FirmwareBundleError'
    this.code = code
  }
}

export async function validateFirmwareBundle(bytes: Uint8Array): Promise<ValidatedFirmwareBundle> {
  if (bytes.byteLength > MAX_BUNDLE_BYTES) {
    throw new FirmwareBundleError('bundle_too_large', 'Firmware bundle exceeds 8 MiB.')
  }
  const centralEntries = readCentralDirectoryEntries(bytes)
  if (
    centralEntries.length !== REQUIRED_PATHS.length ||
    new Set(centralEntries.map((entry) => entry.name)).size !== centralEntries.length
  ) {
    throw new FirmwareBundleError(
      'bundle_entries_invalid',
      'Bundle must contain four unique files.'
    )
  }
  let declaredUnpackedSize = 0
  for (const entry of centralEntries) {
    assertSafePath(entry.name)
    if (entry.encrypted || !entry.regularFile) {
      throw new FirmwareBundleError(
        'bundle_entry_unsupported',
        'Encrypted or non-file ZIP entries are rejected.'
      )
    }
    declaredUnpackedSize += entry.uncompressedSize
    if (declaredUnpackedSize > MAX_BUNDLE_BYTES) {
      throw new FirmwareBundleError(
        'bundle_too_large',
        'Declared uncompressed firmware bundle exceeds 8 MiB.'
      )
    }
  }
  const archive = unzipSync(bytes)
  const names = Object.keys(archive).sort()
  const required = [...REQUIRED_PATHS].sort()
  if (names.length !== required.length || names.some((name, index) => name !== required[index])) {
    throw new FirmwareBundleError('bundle_entries_invalid', 'Bundle file set is not exact.')
  }
  const unpackedSize = names.reduce((total, name) => total + archive[name].byteLength, 0)
  if (unpackedSize > MAX_BUNDLE_BYTES) {
    throw new FirmwareBundleError('bundle_too_large', 'Uncompressed firmware bundle exceeds 8 MiB.')
  }
  let manifest: unknown
  try {
    manifest = JSON.parse(
      new TextDecoder('utf-8', { fatal: true }).decode(archive['manifest.json'])
    )
  } catch {
    throw new FirmwareBundleError('manifest_invalid', 'manifest.json is not strict UTF-8 JSON.')
  }
  if (!validateManifest(manifest)) {
    throw new FirmwareBundleError('manifest_invalid', ajv.errorsText(validateManifest.errors))
  }
  const allowedMigrations = new Set(migrations.migrations.map((migration) => migration.id))
  if (manifest.migrations.some((migration) => !allowedMigrations.has(migration))) {
    throw new FirmwareBundleError('migration_unknown', 'Manifest names an unsupported migration.')
  }
  if (
    manifest.layout.id !== migrations.targetLayoutId ||
    manifest.layout.version !== migrations.targetLayoutVersion ||
    manifest.layout.partitionTableSha256 !== migrations.targetPartitionTableSha256
  ) {
    throw new FirmwareBundleError(
      'layout_hash_mismatch',
      'Bundle layout is not the supported target.'
    )
  }
  for (const segment of manifest.segments) {
    const image = archive[segment.path]
    if (!image || image.byteLength !== segment.length) {
      throw new FirmwareBundleError('segment_length_mismatch', `${segment.kind} length differs.`)
    }
    const sha256 = await digestSha256(image)
    const md5 = SparkMD5.ArrayBuffer.hash(
      image.buffer.slice(image.byteOffset, image.byteOffset + image.byteLength)
    )
    if (segment.sha256 !== `sha256:${sha256}` || segment.md5 !== md5) {
      throw new FirmwareBundleError('segment_hash_mismatch', `${segment.kind} digest differs.`)
    }
  }
  const partition = manifest.segments.find((segment) => segment.kind === 'partition-table')
  if (!partition || partition.sha256 !== manifest.layout.partitionTableSha256) {
    throw new FirmwareBundleError(
      'layout_hash_mismatch',
      'Partition table does not match layout identity.'
    )
  }
  return {
    manifest,
    bundleSha256: `sha256:${await digestSha256(bytes)}`,
    archiveSize: bytes.byteLength,
    images: new Map(
      names.filter((name) => name !== 'manifest.json').map((name) => [name, archive[name]])
    ),
  }
}

async function digestSha256(bytes: Uint8Array) {
  const copy = Uint8Array.from(bytes)
  const digest = await crypto.subtle.digest('SHA-256', copy)
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('')
}

function assertSafePath(name: string) {
  if (!name || name.startsWith('/') || name.includes('\\') || name.includes(':')) {
    throw new FirmwareBundleError('bundle_path_unsafe', `Unsafe ZIP path: ${name}`)
  }
  const parts = name.split('/')
  if (parts.some((part) => part === '' || part === '.' || part === '..')) {
    throw new FirmwareBundleError('bundle_path_unsafe', `Unsafe ZIP path: ${name}`)
  }
}

function readCentralDirectoryEntries(bytes: Uint8Array) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const decoder = new TextDecoder('utf-8', { fatal: true })
  const entries: Array<{
    name: string
    uncompressedSize: number
    encrypted: boolean
    regularFile: boolean
  }> = []
  for (let offset = 0; offset + 46 <= bytes.byteLength; ) {
    if (view.getUint32(offset, true) !== 0x02014b50) {
      offset += 1
      continue
    }
    const nameLength = view.getUint16(offset + 28, true)
    const extraLength = view.getUint16(offset + 30, true)
    const commentLength = view.getUint16(offset + 32, true)
    const end = offset + 46 + nameLength + extraLength + commentLength
    if (end > bytes.byteLength) {
      throw new FirmwareBundleError('bundle_zip_invalid', 'Truncated ZIP central directory.')
    }
    try {
      const name = decoder.decode(bytes.subarray(offset + 46, offset + 46 + nameLength))
      const flags = view.getUint16(offset + 8, true)
      const madeBy = view.getUint16(offset + 4, true) >> 8
      const externalAttributes = view.getUint32(offset + 38, true)
      const unixType = (externalAttributes >>> 16) & 0o170000
      entries.push({
        name,
        uncompressedSize: view.getUint32(offset + 24, true),
        encrypted: (flags & 0x1) !== 0,
        regularFile: madeBy !== 3 || unixType === 0 || unixType === 0o100000,
      })
    } catch {
      throw new FirmwareBundleError('bundle_path_unsafe', 'ZIP entry name is not UTF-8.')
    }
    offset = end
  }
  return entries
}
