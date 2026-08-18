/// <reference types="vitest/config" />

// https://vite.dev/config/
import { createHash } from 'node:crypto'
import type { Dirent } from 'node:fs'
import { readdir, readFile, stat } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { storybookTest } from '@storybook/addon-vitest/vitest-plugin'
import tailwindcss from '@tailwindcss/vite'
import { tanstackRouter } from '@tanstack/router-plugin/vite'
import react from '@vitejs/plugin-react'
import { playwright } from '@vitest/browser-playwright'
import { unzipSync } from 'fflate'
import { defineConfig, type Plugin } from 'vite'

const dirname =
  typeof __dirname !== 'undefined' ? __dirname : path.dirname(fileURLToPath(import.meta.url))
const firmwareLocalRoot = path.resolve(
  dirname,
  process.env.FLUX_PURR_FIRMWARE_ARTIFACTS_DIR ?? '../firmware/target/flux-purr-web-artifacts'
)
const staticFirmwareRoot = path.resolve(dirname, 'public/firmware')
const firmwareReleaseRepository =
  process.env.FLUX_PURR_FIRMWARE_RELEASE_REPOSITORY ?? 'IvanLi-CN/flux-purr'
const firmwareReleaseApi = process.env.FLUX_PURR_FIRMWARE_RELEASE_API ?? 'https://api.github.com'
const firmwareReleaseProxyTtlMs = 5 * 60 * 1000
const maxFirmwareBundleBytes = 8 * 1024 * 1024
const firmwareAssetPath =
  /^firmware\/releases\/[A-Za-z0-9][A-Za-z0-9._-]{0,127}\/[A-Za-z0-9][A-Za-z0-9._-]{0,127}\.fluxpurr-fw$/

type FirmwareBundleIdentity = {
  version: string
  channel: 'stable' | 'rc' | 'local'
  sourceSha: string
  buildId: string
}

type DevFirmwareArtifact = {
  id: string
  version: string
  channel: 'stable' | 'rc' | 'local'
  source: 'release' | 'local'
  releaseTag: string | null
  publishedAt: string
  sourceSha: string
  buildId: string
  bundleSha256: string
  size: number
  assetPath: string
  target: 'ESP32-S3FH4R2'
}

type DevFirmwareProxyIndex = {
  manifest: {
    schemaVersion: 1
    generatedAt: string
    releaseCount: number
    releases: DevFirmwareArtifact[]
  }
  bundles: Map<string, Uint8Array>
}

type GitHubRelease = {
  id: number
  tag_name: string
  draft: boolean
  prerelease: boolean
  published_at: string
  assets: Array<{ id: number; name: string; url: string }>
}

function firmwareHash(bytes: Uint8Array) {
  return `sha256:${createHash('sha256').update(bytes).digest('hex')}`
}

function routeComponent(value: string) {
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(value)) {
    throw new Error('Firmware release contains an unsupported path component.')
  }
  return value
}

function parseFirmwareBundleIdentity(bytes: Uint8Array): FirmwareBundleIdentity {
  if (bytes.byteLength > maxFirmwareBundleBytes) {
    throw new Error('Firmware bundle exceeds 8 MiB.')
  }
  const archive = unzipSync(bytes)
  const expectedEntries = [
    'images/bootloader.bin',
    'images/factory-app.bin',
    'images/partition-table.bin',
    'manifest.json',
  ]
  const names = Object.keys(archive).sort()
  if (
    names.length !== expectedEntries.length ||
    names.some((name, index) => name !== expectedEntries[index])
  ) {
    throw new Error('Firmware bundle entries are invalid.')
  }
  const unpackedSize = Object.values(archive).reduce((total, entry) => total + entry.byteLength, 0)
  if (unpackedSize > maxFirmwareBundleBytes) {
    throw new Error('Firmware bundle unpacked size exceeds 8 MiB.')
  }
  const manifest = JSON.parse(
    new TextDecoder('utf-8', { fatal: true }).decode(archive['manifest.json'])
  ) as {
    identity?: { version?: unknown; channel?: unknown; sourceSha?: unknown; buildId?: unknown }
    target?: { chip?: unknown; package?: unknown }
  }
  const identity = manifest.identity
  if (
    !isFirmwareBundleIdentity(identity) ||
    manifest.target?.chip !== 'esp32s3' ||
    manifest.target?.package !== 'ESP32-S3FH4R2'
  ) {
    throw new Error('Firmware bundle manifest is invalid.')
  }
  return identity
}

function isFirmwareBundleIdentity(value: unknown): value is FirmwareBundleIdentity {
  if (!value || typeof value !== 'object') return false
  const identity = value as Record<string, unknown>
  return (
    typeof identity.version === 'string' &&
    /^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/.test(identity.version) &&
    typeof identity.sourceSha === 'string' &&
    /^[0-9a-f]{40}$/.test(identity.sourceSha) &&
    typeof identity.buildId === 'string' &&
    /^[0-9a-f]{16,64}$/.test(identity.buildId) &&
    (identity.channel === 'stable' || identity.channel === 'rc' || identity.channel === 'local')
  )
}

function releaseAssetPath(tag: string, bundleSha256: string, fileName: string) {
  const safeTag = routeComponent(tag)
  const safeFileName = routeComponent(fileName)
  return `firmware/releases/${safeTag}-${bundleSha256.slice('sha256:'.length, 16 + 'sha256:'.length)}/${safeFileName}`
}

function bundleKey(identity: { sourceSha: string; buildId: string }) {
  return `${identity.sourceSha}:${identity.buildId}`
}

function hasExactKeys(value: Record<string, unknown>, keys: string[]) {
  const actualKeys = Object.keys(value).sort()
  return actualKeys.length === keys.length && actualKeys.every((key, index) => key === keys[index])
}

async function requestGitHubJson<T>(url: string): Promise<T> {
  const token = process.env.GITHUB_TOKEN
  const response = await fetch(url, {
    headers: {
      Accept: 'application/vnd.github+json',
      'User-Agent': 'flux-purr-vite-firmware-proxy',
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
  })
  if (!response.ok) throw new Error(`GitHub firmware catalog failed (${response.status}).`)
  return (await response.json()) as T
}

async function listGitHubReleases(): Promise<GitHubRelease[]> {
  const [owner, repository] = firmwareReleaseRepository.split('/')
  if (!owner || !repository) throw new Error('FLUX_PURR_FIRMWARE_RELEASE_REPOSITORY is invalid.')
  const releases: GitHubRelease[] = []
  for (let page = 1; ; page += 1) {
    const url = `${firmwareReleaseApi.replace(/\/$/, '')}/repos/${owner}/${repository}/releases?per_page=100&page=${page}`
    const batch = await requestGitHubJson<unknown>(url)
    if (!Array.isArray(batch)) throw new Error('GitHub firmware catalog response is invalid.')
    if (batch.length === 0) return releases
    releases.push(...batch.filter(isGitHubRelease))
  }
}

function isGitHubRelease(value: unknown): value is GitHubRelease {
  if (!value || typeof value !== 'object') return false
  const release = value as Record<string, unknown>
  return (
    typeof release.id === 'number' &&
    typeof release.tag_name === 'string' &&
    typeof release.draft === 'boolean' &&
    typeof release.prerelease === 'boolean' &&
    typeof release.published_at === 'string' &&
    Array.isArray(release.assets) &&
    release.assets.every(
      (asset) =>
        asset &&
        typeof asset === 'object' &&
        typeof (asset as Record<string, unknown>).id === 'number' &&
        typeof (asset as Record<string, unknown>).name === 'string' &&
        typeof (asset as Record<string, unknown>).url === 'string'
    )
  )
}

async function downloadGitHubAsset(assetApiUrl: string): Promise<Uint8Array> {
  const token = process.env.GITHUB_TOKEN
  const response = await fetch(assetApiUrl, {
    headers: {
      Accept: 'application/octet-stream',
      'User-Agent': 'flux-purr-vite-firmware-proxy',
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
  })
  if (!response.ok) throw new Error(`GitHub firmware download failed (${response.status}).`)
  const bytes = new Uint8Array(await response.arrayBuffer())
  if (bytes.byteLength > maxFirmwareBundleBytes)
    throw new Error('GitHub firmware bundle exceeds 8 MiB.')
  return bytes
}

async function localFirmwareArtifacts(): Promise<
  Array<{ entry: DevFirmwareArtifact; bytes: Uint8Array }>
> {
  let directoryEntries: Dirent<string>[]
  try {
    directoryEntries = await readdir(firmwareLocalRoot, { withFileTypes: true })
  } catch {
    return []
  }
  const entries: Array<{ entry: DevFirmwareArtifact; bytes: Uint8Array }> = []
  for (const directoryEntry of directoryEntries) {
    if (!directoryEntry.isFile() || !directoryEntry.name.endsWith('.fluxpurr-fw')) continue
    const fileName = routeComponent(directoryEntry.name)
    const filePath = path.join(firmwareLocalRoot, fileName)
    const [bytes, fileStat] = await Promise.all([readFile(filePath), stat(filePath)])
    const identity = parseFirmwareBundleIdentity(bytes)
    const bundleSha256 = firmwareHash(bytes)
    entries.push({
      entry: {
        id: `local:${bundleSha256.slice('sha256:'.length, 16 + 'sha256:'.length)}`,
        version: identity.version,
        channel: identity.channel,
        source: 'local',
        releaseTag: null,
        publishedAt: fileStat.mtime.toISOString(),
        sourceSha: identity.sourceSha,
        buildId: identity.buildId,
        bundleSha256,
        size: bytes.byteLength,
        assetPath: releaseAssetPath('local', bundleSha256, fileName),
        target: 'ESP32-S3FH4R2',
      },
      bytes,
    })
  }
  return entries
}

function isStaticFirmwareArtifact(value: unknown): value is DevFirmwareArtifact {
  if (!value || typeof value !== 'object') return false
  const entry = value as Record<string, unknown>
  return (
    hasExactKeys(entry, [
      'assetPath',
      'buildId',
      'bundleSha256',
      'channel',
      'id',
      'publishedAt',
      'releaseTag',
      'size',
      'source',
      'sourceSha',
      'target',
      'version',
    ]) &&
    typeof entry.id === 'string' &&
    typeof entry.version === 'string' &&
    (entry.channel === 'stable' || entry.channel === 'rc' || entry.channel === 'local') &&
    (entry.source === 'release' || entry.source === 'local') &&
    (typeof entry.releaseTag === 'string' || entry.releaseTag === null) &&
    typeof entry.publishedAt === 'string' &&
    typeof entry.sourceSha === 'string' &&
    /^[0-9a-f]{40}$/.test(entry.sourceSha) &&
    typeof entry.buildId === 'string' &&
    /^[0-9a-f]{16,64}$/.test(entry.buildId) &&
    typeof entry.bundleSha256 === 'string' &&
    /^sha256:[0-9a-f]{64}$/.test(entry.bundleSha256) &&
    typeof entry.size === 'number' &&
    Number.isInteger(entry.size) &&
    entry.size > 0 &&
    entry.size <= maxFirmwareBundleBytes &&
    typeof entry.assetPath === 'string' &&
    firmwareAssetPath.test(entry.assetPath) &&
    entry.target === 'ESP32-S3FH4R2' &&
    !(
      entry.source === 'release' &&
      (entry.channel === 'local' || typeof entry.releaseTag !== 'string')
    ) &&
    !(entry.source === 'local' && (entry.channel !== 'local' || entry.releaseTag !== null))
  )
}

async function staticFirmwareArtifacts(): Promise<
  Array<{ entry: DevFirmwareArtifact; bytes: Uint8Array }>
> {
  const manifestPath = path.join(staticFirmwareRoot, 'releases-manifest.json')
  let payload: unknown
  try {
    payload = JSON.parse(await readFile(manifestPath, 'utf8')) as unknown
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return []
    throw new Error(`Bundled firmware catalog cannot be read: ${String(error)}`)
  }
  if (!payload || typeof payload !== 'object') {
    throw new Error('Bundled firmware catalog is invalid.')
  }
  const catalog = payload as Record<string, unknown>
  if (
    !hasExactKeys(catalog, ['generatedAt', 'releaseCount', 'releases', 'schemaVersion']) ||
    catalog.schemaVersion !== 1 ||
    !Array.isArray(catalog.releases)
  ) {
    throw new Error('Bundled firmware catalog schema is unsupported.')
  }
  if (catalog.releaseCount !== catalog.releases.length) {
    throw new Error('Bundled firmware catalog release count does not match its entries.')
  }
  const artifacts: Array<{ entry: DevFirmwareArtifact; bytes: Uint8Array }> = []
  for (const rawEntry of catalog.releases) {
    if (!isStaticFirmwareArtifact(rawEntry)) {
      throw new Error('Bundled firmware catalog contains an invalid entry.')
    }
    const relativeAssetPath = rawEntry.assetPath.replace(/^firmware\//, '')
    const assetPath = path.resolve(staticFirmwareRoot, relativeAssetPath)
    const relative = path.relative(staticFirmwareRoot, assetPath)
    if (!relative || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
      throw new Error('Bundled firmware catalog path escapes its static directory.')
    }
    const bytes = await readFile(assetPath)
    if (bytes.byteLength !== rawEntry.size || firmwareHash(bytes) !== rawEntry.bundleSha256) {
      throw new Error('Bundled firmware catalog checksum does not match its bundle.')
    }
    const identity = parseFirmwareBundleIdentity(bytes)
    if (
      identity.version !== rawEntry.version ||
      identity.channel !== rawEntry.channel ||
      identity.sourceSha !== rawEntry.sourceSha ||
      identity.buildId !== rawEntry.buildId
    ) {
      throw new Error('Bundled firmware catalog identity does not match its bundle.')
    }
    artifacts.push({ entry: rawEntry, bytes })
  }
  return artifacts
}

async function githubFirmwareArtifacts(): Promise<
  Array<{ entry: DevFirmwareArtifact; bytes: Uint8Array }>
> {
  if (process.env.FLUX_PURR_DEV_FIRMWARE_RELEASES === '0') return []
  const entries: Array<{ entry: DevFirmwareArtifact; bytes: Uint8Array }> = []
  for (const release of await listGitHubReleases()) {
    try {
      if (release.draft) continue
      const asset = release.assets.find((candidate) => candidate.name.endsWith('.fluxpurr-fw'))
      if (!asset) continue
      const tag = routeComponent(release.tag_name)
      const fileName = routeComponent(asset.name)
      const bytes = await downloadGitHubAsset(asset.url)
      const identity = parseFirmwareBundleIdentity(bytes)
      if (identity.channel === 'local') continue
      const bundleSha256 = firmwareHash(bytes)
      entries.push({
        entry: {
          id: `release:${release.id}:${asset.id}`,
          version: identity.version,
          channel: identity.channel,
          source: 'release',
          releaseTag: tag,
          publishedAt: release.published_at,
          sourceSha: identity.sourceSha,
          buildId: identity.buildId,
          bundleSha256,
          size: bytes.byteLength,
          assetPath: releaseAssetPath(tag, bundleSha256, fileName),
          target: 'ESP32-S3FH4R2',
        },
        bytes,
      })
    } catch (error) {
      console.warn(
        `flux-purr dev firmware proxy: skipped invalid release ${release.tag_name}. ${
          error instanceof Error ? error.message : String(error)
        }`
      )
    }
  }
  return entries
}

function devFirmwarePlugin(): Plugin {
  let cache: { expiresAt: number; index: DevFirmwareProxyIndex } | null = null

  const buildIndex = async (): Promise<DevFirmwareProxyIndex> => {
    const [bundled, local] = await Promise.all([
      staticFirmwareArtifacts(),
      localFirmwareArtifacts(),
    ])
    let released: Array<{ entry: DevFirmwareArtifact; bytes: Uint8Array }> = []
    try {
      released = await githubFirmwareArtifacts()
    } catch (error) {
      console.warn(
        `flux-purr dev firmware proxy: GitHub release refresh unavailable; using bundled and local artifacts. ${
          error instanceof Error ? error.message : String(error)
        }`
      )
    }
    const byBundle = new Map<string, { entry: DevFirmwareArtifact; bytes: Uint8Array }>()
    for (const candidate of bundled) byBundle.set(bundleKey(candidate.entry), candidate)
    for (const candidate of released) byBundle.set(bundleKey(candidate.entry), candidate)
    for (const candidate of local) byBundle.set(bundleKey(candidate.entry), candidate)
    const selected = [...byBundle.values()].sort((left, right) =>
      right.entry.publishedAt.localeCompare(left.entry.publishedAt)
    )
    return {
      manifest: {
        schemaVersion: 1,
        generatedAt: new Date().toISOString(),
        releaseCount: selected.length,
        releases: selected.map((candidate) => candidate.entry),
      },
      bundles: new Map(selected.map((candidate) => [candidate.entry.assetPath, candidate.bytes])),
    }
  }

  return {
    name: 'flux-purr-dev-firmware-proxy',
    apply: 'serve',
    configureServer(server) {
      const invalidateLocalArtifacts = (filePath: string) => {
        const relative = path.relative(firmwareLocalRoot, path.resolve(filePath))
        if (
          relative === '' ||
          (!relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative))
        ) {
          cache = null
        }
      }
      server.watcher.add(firmwareLocalRoot)
      server.watcher.on('add', invalidateLocalArtifacts)
      server.watcher.on('change', invalidateLocalArtifacts)
      server.watcher.on('unlink', invalidateLocalArtifacts)
      server.middlewares.use(async (request, response, next) => {
        const requestPath = decodeURIComponent((request.url ?? '').split('?', 1)[0] ?? '')
        if (!requestPath.startsWith('/firmware/')) {
          next()
          return
        }
        try {
          const now = Date.now()
          if (!cache || cache.expiresAt <= now) {
            cache = { expiresAt: now + firmwareReleaseProxyTtlMs, index: await buildIndex() }
          }
          if (requestPath === '/firmware/releases-manifest.json') {
            response.statusCode = 200
            response.setHeader('Content-Type', 'application/json; charset=utf-8')
            response.end(JSON.stringify(cache.index.manifest))
            return
          }
          const bundle = cache.index.bundles.get(requestPath.slice(1))
          if (bundle) {
            response.statusCode = 200
            response.setHeader('Content-Type', 'application/vnd.flux-purr.firmware-bundle+zip')
            response.setHeader('Content-Length', bundle.byteLength)
            response.end(bundle)
            return
          }
        } catch (error) {
          server.config.logger.warn(
            `dev firmware proxy failed: ${error instanceof Error ? error.message : String(error)}`
          )
          response.statusCode = 502
          response.setHeader('Content-Type', 'application/json; charset=utf-8')
          response.end(JSON.stringify({ error: 'firmware_release_proxy_failed' }))
          return
        }
        next()
      })
    },
  }
}

// More info at: https://storybook.js.org/docs/next/writing-tests/integrations/vitest-addon
export default defineConfig({
  plugins: [
    tanstackRouter({ target: 'react', autoCodeSplitting: true }),
    devFirmwarePlugin(),
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      '@': path.resolve(dirname, './src'),
    },
  },
  test: {
    projects: [
      {
        extends: true,
        plugins: [
          // The plugin will run tests for the stories defined in your Storybook config
          // See options at: https://storybook.js.org/docs/next/writing-tests/integrations/vitest-addon#storybooktest
          storybookTest({
            configDir: path.join(dirname, '.storybook'),
          }),
        ],
        test: {
          name: 'storybook',
          browser: {
            enabled: true,
            headless: true,
            provider: playwright({}),
            instances: [
              {
                browser: 'chromium',
              },
            ],
          },
          setupFiles: ['.storybook/vitest.setup.ts'],
        },
      },
    ],
  },
})
