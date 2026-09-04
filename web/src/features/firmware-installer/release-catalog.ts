import Ajv2020 from 'ajv/dist/2020'
import integritySchema from '../../../../docs/specs/web-firmware-install-recovery/contracts/firmware-integrity-catalog.schema.json'
import schema from '../../../../docs/specs/web-firmware-install-recovery/contracts/firmware-release-catalog.schema.json'
import { validateFirmwareBundle } from './bundle'
import type { FirmwareChannel, ValidatedFirmwareBundle } from './types'

const CATALOG_PATH = 'firmware/releases-manifest.json'
const INTEGRITY_CATALOG_PATH = 'firmware/firmware-integrity-catalog.json'
const ajv = new Ajv2020({ allErrors: true, strict: true })
const validateCatalogSchema = ajv.compile<SameOriginReleaseCatalog>(schema)
const validateIntegrityCatalogSchema = ajv.compile<SameOriginIntegrityCatalog>(integritySchema)

export type FirmwareCatalogSource = 'release' | 'local'

interface SameOriginReleaseCatalog {
  schemaVersion: 1
  generatedAt: string
  releaseCount: number
  releases: FirmwareReleaseCatalogEntry[]
}

interface FirmwareReleaseCatalogEntry {
  id: string
  version: string
  channel: FirmwareChannel
  source: FirmwareCatalogSource
  releaseTag: string | null
  sourceSha: string
  buildId: string
  bundleSha256: string
  size: number
  assetPath: string
  target: 'ESP32-S3FH4R2'
  publishedAt: string
}

interface SameOriginIntegrityCatalog {
  schemaVersion: 1
  bundles: Array<{
    version: string
    sourceSha: string
    buildId: string
    channel: 'stable' | 'rc'
    hardwareProfile: 'ESP32-S3FH4R2'
    bundleSha256: string
  }>
}

export interface OfficialFirmwareArtifact {
  id: string
  version: string
  channel: FirmwareChannel
  source: FirmwareCatalogSource
  releaseTag: string | null
  sourceSha: string
  buildId: string
  assetPath: string
  bundleSha256: string
  target: 'ESP32-S3FH4R2'
  publishedAt: string
}

function appAssetPath(path: string): string {
  const base = import.meta.env.BASE_URL.endsWith('/')
    ? import.meta.env.BASE_URL
    : `${import.meta.env.BASE_URL}/`
  return `${base}${path}`
}

function parseCatalogEntry(entry: FirmwareReleaseCatalogEntry): OfficialFirmwareArtifact {
  return {
    id: entry.id,
    version: entry.version,
    channel: entry.channel,
    source: entry.source,
    releaseTag: entry.releaseTag,
    sourceSha: entry.sourceSha,
    buildId: entry.buildId,
    assetPath: appAssetPath(entry.assetPath),
    bundleSha256: entry.bundleSha256,
    target: entry.target,
    publishedAt: entry.publishedAt,
  }
}

function parseCatalog(payload: unknown): OfficialFirmwareArtifact[] {
  if (!validateCatalogSchema(payload)) {
    throw new Error(
      `Same-origin firmware catalog is invalid: ${ajv.errorsText(validateCatalogSchema.errors)}`
    )
  }
  const catalog = payload as SameOriginReleaseCatalog
  const artifacts = catalog.releases.map(parseCatalogEntry)
  if (artifacts.length !== catalog.releaseCount) {
    throw new Error('Same-origin firmware catalog release count does not match its entries.')
  }
  return artifacts.sort((left, right) => right.publishedAt.localeCompare(left.publishedAt))
}

function parseIntegrityCatalog(payload: unknown): SameOriginIntegrityCatalog {
  if (!validateIntegrityCatalogSchema(payload)) {
    throw new Error(
      `Same-origin firmware integrity catalog is invalid: ${ajv.errorsText(validateIntegrityCatalogSchema.errors)}`
    )
  }
  const catalog = payload as SameOriginIntegrityCatalog
  const hashes = new Set<string>()
  for (const entry of catalog.bundles) {
    if (hashes.has(entry.bundleSha256)) {
      throw new Error('Same-origin firmware integrity catalog contains duplicate bundle hashes.')
    }
    hashes.add(entry.bundleSha256)
  }
  return catalog
}

export async function fetchOfficialCatalog(): Promise<OfficialFirmwareArtifact[]> {
  const [response, integrityResponse] = await Promise.all([
    fetch(appAssetPath(CATALOG_PATH), {
      cache: 'no-store',
      headers: { Accept: 'application/json' },
    }),
    fetch(appAssetPath(INTEGRITY_CATALOG_PATH), {
      cache: 'no-store',
      headers: { Accept: 'application/json' },
    }),
  ])
  if (!response.ok) throw new Error(`Same-origin firmware catalog failed (${response.status}).`)
  if (!integrityResponse.ok) {
    throw new Error(`Same-origin firmware integrity catalog failed (${integrityResponse.status}).`)
  }
  const artifacts = parseCatalog((await response.json()) as unknown)
  const integrity = parseIntegrityCatalog((await integrityResponse.json()) as unknown)
  const byHash = new Map(integrity.bundles.map((entry) => [entry.bundleSha256, entry]))
  for (const artifact of artifacts.filter((candidate) => candidate.source === 'release')) {
    const entry = byHash.get(artifact.bundleSha256)
    if (
      !entry ||
      entry.version !== artifact.version ||
      entry.sourceSha !== artifact.sourceSha ||
      entry.buildId !== artifact.buildId ||
      entry.channel !== artifact.channel ||
      entry.hardwareProfile !== artifact.target
    ) {
      throw new Error('Same-origin firmware release is missing from the integrity catalog.')
    }
  }
  return artifacts
}

export async function fetchOfficialBundle(
  artifact: OfficialFirmwareArtifact
): Promise<{ bytes: Uint8Array; bundle: ValidatedFirmwareBundle }> {
  const response = await fetch(artifact.assetPath, {
    cache: 'no-store',
    headers: { Accept: 'application/octet-stream' },
  })
  if (!response.ok) {
    throw new Error(`Same-origin firmware download failed (${response.status}).`)
  }
  const bytes = new Uint8Array(await response.arrayBuffer())
  const bundle = await validateFirmwareBundle(bytes)
  if (bundle.bundleSha256 !== artifact.bundleSha256) {
    throw new Error('Same-origin firmware bundle hash does not match the selected catalog.')
  }
  if (bundle.manifest.identity.version !== artifact.version) {
    throw new Error('Same-origin firmware bundle version does not match the selected catalog.')
  }
  if (bundle.manifest.identity.channel !== artifact.channel) {
    throw new Error('Same-origin firmware bundle channel does not match the selected catalog.')
  }
  if (bundle.manifest.identity.sourceSha !== artifact.sourceSha) {
    throw new Error('Same-origin firmware bundle source SHA does not match the selected catalog.')
  }
  if (bundle.manifest.identity.buildId !== artifact.buildId) {
    throw new Error('Same-origin firmware bundle build ID does not match the selected catalog.')
  }
  return { bytes, bundle }
}
