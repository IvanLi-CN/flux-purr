import type { ControlPlaneStatus, Identity, NetworkSummary } from './contracts'
import { ControlPlaneClientError } from './transport-client'
import type { DeviceTarget } from './types'

const LAN_STORAGE_PREFIX = 'flux-purr:lan-device:'
const LAN_ADDRESS_STORAGE_KEY = 'flux-purr:lan-address'
const LAN_SCAN_CIDR_STORAGE_KEY = 'flux-purr:lan-scan-cidr'
const LAN_LEASE_TTL_MS = 30_000

export type LanPairingMode = 'required' | 'optional' | 'unavailable'

export interface LanPairingMetadata {
  mode: LanPairingMode
  active: boolean
  attemptsRemaining: number
}

export interface LanPublicInfo {
  api: string
  deviceId: string
  hostname: string
  firmwareVersion: string
  pairing: LanPairingMetadata
}

export interface DiscoveredLanDevice {
  baseUrl: string
  info: LanPublicInfo
}

export interface LanSubnetScanOptions {
  fetcher?: typeof fetch
  concurrency?: number
  timeoutMs?: number
  signal?: AbortSignal
  onProgress?: (progress: { done: number; total: number }) => void
}

export interface LanDeviceSession {
  baseUrl: string
  token: string
  hostname?: string
  controlRevision?: number
}

export interface LanLease {
  leaseId: string
  ttlMs: number
}

export interface LanProbe {
  identity: Identity
  network: NetworkSummary
  status: ControlPlaneStatus
}

export type LanProbeMode = 'concurrent' | 'serial'

export function isDirectLanDevice(device: Pick<DeviceTarget, 'baseUrl' | 'transport'>) {
  return device.transport === 'wifi' && device.baseUrl.startsWith('http://')
}

export function lanProbeToDeviceTarget(session: LanDeviceSession, probe: LanProbe): DeviceTarget {
  const { identity, network, status } = probe
  return {
    id: `lan-${identity.deviceId}`,
    identityId: identity.deviceId,
    alias: identity.hostname || session.hostname || identity.deviceId,
    location: network.ip ?? session.baseUrl,
    transport: 'wifi',
    severity: network.state === 'error' || network.state === 'timeout' ? 'warning' : 'nominal',
    baseUrl: session.baseUrl,
    firmware: identity.firmwareVersion,
    buildId: identity.buildId,
    uptime: formatUptime(status.uptimeSeconds),
    boardTempC: status.boardTempCenti / 100,
    currentTempC: status.currentTempC,
    targetTempC: status.targetTempC,
    selectedPresetIndex: status.selectedPresetSlot,
    presetsC: status.presetsC,
    rtdRawAdcMv: status.rtdRawAdcMv,
    vinRawAdcMv: status.vinRawAdcMv,
    voltageMv: status.voltageMv,
    currentMa: status.currentMa,
    pdRequestMv: status.pdRequestMv,
    pdContractMv: status.pdContractMv,
    pdState: status.pdState,
    manualPpsEnabled: status.manualPpsEnabled ?? false,
    manualPpsMv: status.manualPpsMv ?? null,
    manualPpsMa: status.manualPpsMa ?? null,
    ppsCapabilityMinMv: status.ppsCapabilityMinMv ?? null,
    ppsCapabilityMaxMv: status.ppsCapabilityMaxMv ?? null,
    ppsCapabilityMaxMa: status.ppsCapabilityMaxMa ?? null,
    manualPpsError: status.manualPpsError ?? null,
    faultAttentionPending: status.faultAttentionPending ?? false,
    heaterLockReason: status.heaterLockReason ?? null,
    calibration: status.calibration,
    heaterEnabled: status.heaterEnabled,
    heaterOutputPercent: status.heaterOutputPercent,
    activeCoolingEnabled: status.activeCoolingEnabled,
    fanState: status.fanDisplayState,
    wifiSsid: network.ssid ?? null,
    wifiRssi: network.wifiRssi ?? null,
    wifiPasswordLength: network.wifiPasswordLength ?? 0,
    capabilities: Array.from(new Set([...identity.capabilities, 'lan_http', 'lan_lease'])),
    networkState: network.state,
    leaseState: 'none',
  }
}

/**
 * Reconciles a direct-LAN target by stable device identity rather than its
 * current DHCP address. A stale record with the same address is also removed
 * so a reused address cannot leave duplicate browser targets behind.
 */
export function upsertLanDeviceTarget(devices: DeviceTarget[], target: DeviceTarget) {
  let replaced = false
  const next: DeviceTarget[] = []

  for (const device of devices) {
    const sameIdentity = device.id === target.id
    const sameAddress = isDirectLanDevice(device) && device.baseUrl === target.baseUrl
    if (!sameIdentity && !sameAddress) {
      next.push(device)
      continue
    }
    if (!replaced) {
      next.push(target)
      replaced = true
    }
  }

  if (!replaced) {
    next.push(target)
  }
  return next
}

type PrivateNetworkRequestInit = RequestInit & {
  targetAddressSpace?: 'private'
}

export function isChromiumPrivateNetworkSupported(userAgent = getUserAgent()) {
  return /(?:Chrome|Chromium|Edg)\//.test(userAgent) && !/Version\/.*Safari\//.test(userAgent)
}

export function normalizeLanBaseUrl(raw: string) {
  const url = new URL(raw.trim())
  if (
    url.protocol !== 'http:' ||
    !url.hostname ||
    !isPrivateLanHostname(url.hostname) ||
    url.username ||
    url.password ||
    (url.pathname !== '/' && url.pathname !== '') ||
    url.search ||
    url.hash
  ) {
    throw new ControlPlaneClientError('请输入设备的 HTTP 根地址。', 'lan_url_invalid', false)
  }
  return url.toString().replace(/\/$/, '')
}

export async function scanLanSubnet(
  cidr: string,
  {
    fetcher = fetch,
    concurrency = 12,
    timeoutMs = 900,
    signal,
    onProgress,
  }: LanSubnetScanOptions = {}
): Promise<DiscoveredLanDevice[]> {
  const hosts = parsePrivateLanCidr(cidr)
  const devices = new Map<string, DiscoveredLanDevice>()
  let nextIndex = 0
  let done = 0

  const worker = async () => {
    for (;;) {
      if (signal?.aborted) return
      const index = nextIndex
      nextIndex += 1
      if (index >= hosts.length) return

      const baseUrl = `http://${hosts[index]}`
      try {
        const info = await getLanPublicInfo(baseUrl, withRequestTimeout(fetcher, timeoutMs, signal))
        devices.set(info.deviceId, { baseUrl, info })
      } catch {
        // Unreachable hosts and non-Flux-Purr responses are expected during a scan.
      } finally {
        done += 1
        onProgress?.({ done, total: hosts.length })
      }
    }
  }

  await Promise.all(
    Array.from({ length: Math.min(Math.max(1, Math.floor(concurrency)), hosts.length) }, worker)
  )
  return Array.from(devices.values()).sort((left, right) =>
    left.baseUrl.localeCompare(right.baseUrl, undefined, { numeric: true })
  )
}

function parsePrivateLanCidr(raw: string, maxHosts = 256) {
  const [address, prefixRaw, ...rest] = raw.trim().split('/')
  if (!address || !prefixRaw || rest.length > 0) {
    throw new ControlPlaneClientError(
      'CIDR 格式应类似 192.168.1.0/24。',
      'lan_scan_cidr_invalid',
      false
    )
  }
  const octets = address.split('.').map(Number)
  const prefix = Number(prefixRaw)
  if (
    octets.length !== 4 ||
    octets.some((value) => !Number.isInteger(value) || value < 0 || value > 255) ||
    !Number.isInteger(prefix) ||
    prefix < 0 ||
    prefix > 32
  ) {
    throw new ControlPlaneClientError(
      'CIDR 格式应类似 192.168.1.0/24。',
      'lan_scan_cidr_invalid',
      false
    )
  }
  if (!isPrivateLanHostname(address)) {
    throw new ControlPlaneClientError('只能扫描私有 IPv4 网段。', 'lan_scan_cidr_public', false)
  }

  const size = 2 ** (32 - prefix)
  if (size > maxHosts) {
    throw new ControlPlaneClientError(
      `扫描范围最多包含 ${maxHosts} 个地址。`,
      'lan_scan_cidr_too_large',
      false
    )
  }
  const value = ((octets[0] << 24) | (octets[1] << 16) | (octets[2] << 8) | octets[3]) >>> 0
  const mask = prefix === 0 ? 0 : (0xffffffff << (32 - prefix)) >>> 0
  const network = (value & mask) >>> 0
  const skipNetworkAndBroadcast = prefix <= 30 && size >= 4
  const hosts: string[] = []
  for (let offset = 0; offset < size; offset += 1) {
    if (skipNetworkAndBroadcast && (offset === 0 || offset === size - 1)) continue
    const host = (network + offset) >>> 0
    hosts.push([host >>> 24, (host >>> 16) & 0xff, (host >>> 8) & 0xff, host & 0xff].join('.'))
  }
  return hosts
}

function withRequestTimeout(
  fetcher: typeof fetch,
  timeoutMs: number,
  parentSignal?: AbortSignal
): typeof fetch {
  return async (input, init) => {
    const controller = new AbortController()
    const abort = () => controller.abort()
    if (parentSignal?.aborted) {
      controller.abort()
    } else {
      parentSignal?.addEventListener('abort', abort, { once: true })
    }
    const timer = globalThis.setTimeout(() => controller.abort(), Math.max(1, timeoutMs))
    try {
      return await fetcher(input, { ...init, signal: controller.signal })
    } finally {
      globalThis.clearTimeout(timer)
      parentSignal?.removeEventListener('abort', abort)
    }
  }
}

function isPrivateLanHostname(hostname: string) {
  const host = hostname.replace(/\.$/, '').toLowerCase()
  const octets = host.split('.')
  if (octets.length === 4 && octets.every((value) => /^\d+$/.test(value))) {
    const values = octets.map(Number)
    return (
      values.every((value) => value >= 0 && value <= 255) &&
      (values[0] === 10 ||
        (values[0] === 172 && values[1] >= 16 && values[1] <= 31) ||
        (values[0] === 192 && values[1] === 168))
    )
  }
  return /^flux-purr-[a-f0-9]{12}\.local$/.test(host)
}

export async function getLanPairingMetadata(
  baseUrl: string,
  fetcher: typeof fetch = fetch
): Promise<LanPairingMetadata> {
  const metadata = await lanRequest<unknown>(
    fetcher,
    normalizeLanBaseUrl(baseUrl),
    '/api/v1/pairing'
  )
  return parsePairingMetadata(metadata)
}

/**
 * Establishes a public LAN connection before any pairing credential is
 * requested. The summary is intentionally small enough for low-frequency
 * display and never includes operational status, a code, or a bearer token.
 */
export async function getLanPublicInfo(
  baseUrl: string,
  fetcher: typeof fetch = fetch
): Promise<LanPublicInfo> {
  const response = await lanRequest<unknown>(fetcher, normalizeLanBaseUrl(baseUrl), '/health')
  if (!isRecord(response) || response.ok !== true) {
    throw new ControlPlaneClientError(
      '设备返回的公开连接信息无效。',
      'lan_public_info_invalid',
      false
    )
  }
  const pairing = parsePairingMetadata(response.pairing)
  if (
    typeof response.api !== 'string' ||
    typeof response.deviceId !== 'string' ||
    typeof response.hostname !== 'string' ||
    typeof response.firmwareVersion !== 'string'
  ) {
    throw new ControlPlaneClientError(
      '设备返回的公开连接信息无效。',
      'lan_public_info_invalid',
      false
    )
  }
  return {
    api: response.api,
    deviceId: response.deviceId,
    hostname: response.hostname,
    firmwareVersion: response.firmwareVersion,
    pairing,
  }
}

export async function claimLanPairing(
  baseUrl: string,
  code?: string,
  fetcher: typeof fetch = fetch
): Promise<LanDeviceSession> {
  if (code !== undefined && !/^\d{4}$/.test(code)) {
    throw new ControlPlaneClientError('配对码必须是四位数字。', 'pairing_code_invalid', false)
  }
  const normalizedBaseUrl = normalizeLanBaseUrl(baseUrl)
  const response = await lanRequest<{ token: string; hostname?: string }>(
    fetcher,
    normalizedBaseUrl,
    '/api/v1/pairing/claim',
    { method: 'POST', body: JSON.stringify(code === undefined ? {} : { code }) }
  )
  if (!/^[a-f0-9]{64}$/i.test(response.token)) {
    throw new ControlPlaneClientError('设备返回的配对凭据无效。', 'pairing_response_invalid', false)
  }
  const session = { baseUrl: normalizedBaseUrl, token: response.token, hostname: response.hostname }
  storeLanDeviceSession(session)
  return session
}

export function storeLanDeviceSession(session: LanDeviceSession) {
  getStorage()?.setItem(`${LAN_STORAGE_PREFIX}${session.baseUrl}`, JSON.stringify(session))
}

/** Stores the last explicit browser-side device address, never a credential. */
export function storeLanAddress(address: string) {
  const normalized = address.trim()
  const storage = getStorage()
  if (!storage) return
  if (!normalized) {
    storage.removeItem(LAN_ADDRESS_STORAGE_KEY)
    return
  }
  storage.setItem(LAN_ADDRESS_STORAGE_KEY, normalized)
}

export function loadLanAddress(fallback: string) {
  return getStorage()?.getItem(LAN_ADDRESS_STORAGE_KEY)?.trim() || fallback
}

/** Stores only the last explicit browser-side scan range, never a credential or device token. */
export function storeLanScanCidr(cidr: string) {
  const normalized = cidr.trim()
  const storage = getStorage()
  if (!storage) return
  if (!normalized) {
    storage.removeItem(LAN_SCAN_CIDR_STORAGE_KEY)
    return
  }
  storage.setItem(LAN_SCAN_CIDR_STORAGE_KEY, normalized)
}

export function loadLanScanCidr(fallback: string) {
  return getStorage()?.getItem(LAN_SCAN_CIDR_STORAGE_KEY)?.trim() || fallback
}

export function loadLanDeviceSession(baseUrl: string): LanDeviceSession | null {
  const normalized = normalizeLanBaseUrl(baseUrl)
  const raw = getStorage()?.getItem(`${LAN_STORAGE_PREFIX}${normalized}`)
  if (!raw) return null
  try {
    const session = JSON.parse(raw) as LanDeviceSession
    return session.baseUrl === normalized && /^[a-f0-9]{64}$/i.test(session.token) ? session : null
  } catch {
    return null
  }
}

export function listSavedLanDeviceSessions(): LanDeviceSession[] {
  const storage = getStorage()
  if (!storage) return []
  const sessions: LanDeviceSession[] = []
  for (let index = 0; index < storage.length; index += 1) {
    const key = storage.key(index)
    if (!key?.startsWith(LAN_STORAGE_PREFIX)) continue
    const raw = storage.getItem(key)
    if (!raw) continue
    try {
      const session = JSON.parse(raw) as LanDeviceSession
      const baseUrl = normalizeLanBaseUrl(session.baseUrl)
      if (/^[a-f0-9]{64}$/i.test(session.token)) {
        sessions.push({ ...session, baseUrl })
      }
    } catch {
      // Ignore malformed local records rather than letting them block LAN setup.
    }
  }
  return sessions
}

export function forgetLanDeviceSession(baseUrl: string) {
  getStorage()?.removeItem(`${LAN_STORAGE_PREFIX}${normalizeLanBaseUrl(baseUrl)}`)
}

export async function createLanLease(session: LanDeviceSession, fetcher: typeof fetch = fetch) {
  return lanRequest<LanLease>(fetcher, session.baseUrl, '/api/v1/leases', {
    method: 'POST',
    headers: bearerHeaders(session),
  })
}

export async function heartbeatLanLease(
  session: LanDeviceSession,
  leaseId: string,
  fetcher: typeof fetch = fetch
) {
  return lanRequest<LanLease>(fetcher, session.baseUrl, '/api/v1/leases', {
    method: 'PUT',
    headers: { ...bearerHeaders(session), 'X-Flux-Purr-Lease': leaseId },
  })
}

export async function releaseLanLease(
  session: LanDeviceSession,
  leaseId: string,
  fetcher: typeof fetch = fetch
) {
  await lanRequest(fetcher, session.baseUrl, '/api/v1/leases', {
    method: 'DELETE',
    headers: { ...bearerHeaders(session), 'X-Flux-Purr-Lease': leaseId },
  })
}

export function startLanLeaseHeartbeat(
  session: LanDeviceSession,
  lease: LanLease,
  onError: (error: ControlPlaneClientError) => void,
  fetcher: typeof fetch = fetch
) {
  const interval = Math.max(1_000, Math.min(Math.floor(lease.ttlMs / 2), LAN_LEASE_TTL_MS / 2))
  const timer = window.setInterval(() => {
    void heartbeatLanLease(session, lease.leaseId, fetcher).catch((error: unknown) => {
      onError(asControlPlaneError(error))
    })
  }, interval)
  return () => window.clearInterval(timer)
}

export async function probeLanDevice(
  session: LanDeviceSession,
  fetcher: typeof fetch = fetch,
  mode: LanProbeMode = 'concurrent'
): Promise<LanProbe> {
  const headers = bearerHeaders(session)
  const rememberRevision =
    mode === 'serial'
      ? (revision: number) => {
          // A complete serial probe has one ordered response chain ending in
          // status, so it is authoritative across a device reboot where the
          // in-memory control revision restarts at a lower value.
          session.controlRevision = revision
        }
      : (revision: number) => {
          session.controlRevision = Math.max(session.controlRevision ?? 0, revision)
        }
  const persistProbe = (probe: LanProbe) => {
    // The control revision is learned from the device response headers. Keep
    // it with the saved session so a later runtime write cannot be rejected
    // merely because it reloaded the same paired device from local storage.
    storeLanDeviceSession(session)
    return probe
  }
  if (mode === 'serial') {
    return persistProbe(
      await probeLanDeviceRequests(session, fetcher, headers, rememberRevision, false)
    )
  }
  try {
    return persistProbe(
      await probeLanDeviceRequests(session, fetcher, headers, rememberRevision, true)
    )
  } catch (error) {
    if (
      !(error instanceof ControlPlaneClientError) ||
      error.code !== 'lan_private_network_unavailable'
    ) {
      throw error
    }
    // Some shipped firmware revisions accept one TCP request at a time even
    // though the v1 contract permits concurrent reads. Retry only this
    // transport-level failure serially; auth and protocol failures remain
    // terminal and are never hidden by a retry.
    return persistProbe(
      await probeLanDeviceRequests(session, fetcher, headers, rememberRevision, false)
    )
  }
}

async function probeLanDeviceRequests(
  session: LanDeviceSession,
  fetcher: typeof fetch,
  headers: Record<string, string>,
  rememberRevision: (revision: number) => void,
  concurrent: boolean
): Promise<LanProbe> {
  const request = <T>(path: string) =>
    lanRequest<T>(fetcher, session.baseUrl, path, { headers }, rememberRevision)
  if (concurrent) {
    // Firmware exposes two HTTP workers. Start only the two independent reads
    // together, then free a worker before asking the device for runtime status.
    const [identity, network] = await Promise.all([
      request<Identity>('/api/v1/identity'),
      request<NetworkSummary>('/api/v1/network'),
    ])
    const status = await request<ControlPlaneStatus>('/api/v1/status')
    return { identity, network, status }
  }

  const identity = await request<Identity>('/api/v1/identity')
  const network = await request<NetworkSummary>('/api/v1/network')
  const status = await request<ControlPlaneStatus>('/api/v1/status')
  return { identity, network, status }
}

export async function writeLanRuntime(
  session: LanDeviceSession,
  leaseId: string,
  body: Record<string, unknown>,
  fetcher: typeof fetch = fetch
) {
  const revision = requireLanControlRevision(session)
  return lanRequest<ControlPlaneStatus>(
    fetcher,
    session.baseUrl,
    '/api/v1/runtime',
    {
      method: 'PUT',
      headers: {
        ...bearerHeaders(session),
        'X-Flux-Purr-Lease': leaseId,
        'X-Flux-Purr-Revision': String(revision),
      },
      body: JSON.stringify(body),
    },
    (next) => {
      rememberLanControlRevision(session, next)
    }
  )
}

export async function authorizedLanRequest<T = Record<string, unknown>>(
  session: LanDeviceSession,
  path: string,
  method = 'GET',
  body?: Record<string, unknown>,
  leaseId?: string,
  fetcher: typeof fetch = fetch
) {
  const isWrite = method !== 'GET'
  const headers: Record<string, string> = {
    ...bearerHeaders(session),
    ...(leaseId ? { 'X-Flux-Purr-Lease': leaseId } : {}),
  }
  if (isWrite) {
    headers['X-Flux-Purr-Revision'] = String(requireLanControlRevision(session))
  }
  return lanRequest<T>(
    fetcher,
    session.baseUrl,
    `/api/v1/${path.replace(/^\//, '')}`,
    {
      method,
      headers,
      ...(body ? { body: JSON.stringify(body) } : {}),
    },
    (next) => {
      rememberLanControlRevision(session, next)
    }
  )
}

export async function* streamLanEvents(
  session: LanDeviceSession,
  signal?: AbortSignal,
  fetcher: typeof fetch = fetch
): AsyncGenerator<Record<string, unknown>> {
  while (!signal?.aborted) {
    try {
      const request = {
        headers: { ...bearerHeaders(session), Accept: 'text/event-stream' },
        signal,
        targetAddressSpace: 'private',
      } satisfies PrivateNetworkRequestInit
      const response = await fetcher(`${session.baseUrl}/api/v1/events`, request as RequestInit)
      if (!response.ok || !response.body) throw await responseError(response, session.baseUrl)
      const reader = response.body.getReader()
      const decoder = new TextDecoder()
      let buffered = ''
      for (;;) {
        const next = await reader.read()
        if (next.done) break
        buffered += decoder.decode(next.value, { stream: true })
        const frames = buffered.split('\n\n')
        buffered = frames.pop() ?? ''
        for (const frame of frames) {
          const data = frame
            .split('\n')
            .filter((line) => line.startsWith('data:'))
            .map((line) => line.slice(5).trim())
            .join('\n')
          if (!data) continue
          try {
            yield JSON.parse(data) as Record<string, unknown>
          } catch {
            /* Ignore malformed event frames. */
          }
        }
      }
    } catch (error) {
      if (signal?.aborted) return
      if (error instanceof ControlPlaneClientError && !error.retryable) throw error
    }
    if (!signal?.aborted) await delay(1_000, signal)
  }
}

async function lanRequest<T = Record<string, unknown>>(
  fetcher: typeof fetch,
  baseUrl: string,
  path: string,
  init: RequestInit = {},
  onRevision?: (revision: number) => void
): Promise<T> {
  const headers = {
    Accept: 'application/json',
    ...(init.body ? { 'Content-Type': 'application/json' } : {}),
    ...init.headers,
  }
  const request = {
    ...init,
    headers,
    targetAddressSpace: 'private',
  } satisfies PrivateNetworkRequestInit
  let response: Response
  try {
    response = await fetcher(`${baseUrl}${path}`, request as RequestInit)
  } catch (error) {
    if (error instanceof TypeError) {
      throw new ControlPlaneClientError(
        '浏览器无法访问私网设备。',
        'lan_private_network_unavailable',
        false
      )
    }
    throw error
  }
  if (!response.ok) throw await responseError(response, baseUrl)
  const revision = Number(response.headers.get('X-Flux-Purr-Revision'))
  if (Number.isSafeInteger(revision) && revision >= 0) onRevision?.(revision)
  try {
    return (await response.json()) as T
  } catch {
    throw new ControlPlaneClientError('设备返回的数据格式无效。', 'lan_response_invalid', false)
  }
}

function requireLanControlRevision(session: LanDeviceSession) {
  if (!Number.isSafeInteger(session.controlRevision) || (session.controlRevision ?? -1) < 0) {
    throw new ControlPlaneClientError(
      '写入前必须先读取设备最新状态。',
      'lan_revision_required',
      false
    )
  }
  return session.controlRevision as number
}

function rememberLanControlRevision(session: LanDeviceSession, revision: number) {
  session.controlRevision = Math.max(session.controlRevision ?? 0, revision)
  storeLanDeviceSession(session)
}

function bearerHeaders(session: LanDeviceSession) {
  return { Authorization: `Bearer ${session.token}` }
}

async function responseError(response: Response, baseUrl: string) {
  const payload = (await response.json().catch(() => null)) as {
    error?: { code?: string; message?: string; retryable?: boolean }
  } | null
  if (response.status === 401) forgetLanDeviceSession(baseUrl)
  return new ControlPlaneClientError(
    payload?.error?.message ?? `设备请求失败 (${response.status})`,
    payload?.error?.code ?? 'lan_request_failed',
    payload?.error?.retryable ?? response.status >= 500
  )
}

function asControlPlaneError(error: unknown) {
  return error instanceof ControlPlaneClientError
    ? error
    : new ControlPlaneClientError('设备 lease 心跳失败。', 'lan_lease_expired', true)
}

function parsePairingMetadata(value: unknown): LanPairingMetadata {
  const attemptsRemaining = isRecord(value) ? value.attemptsRemaining : undefined
  if (
    !isRecord(value) ||
    !isLanPairingMode(value.mode) ||
    typeof value.active !== 'boolean' ||
    typeof attemptsRemaining !== 'number' ||
    !Number.isInteger(attemptsRemaining) ||
    attemptsRemaining < 0 ||
    attemptsRemaining > 5
  ) {
    throw new ControlPlaneClientError(
      '设备返回的配对策略无效。',
      'lan_pairing_metadata_invalid',
      false
    )
  }
  return {
    mode: value.mode,
    active: value.active,
    attemptsRemaining,
  }
}

function isLanPairingMode(value: unknown): value is LanPairingMode {
  return value === 'required' || value === 'optional' || value === 'unavailable'
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}
function getStorage() {
  return typeof window === 'undefined' ? null : window.localStorage
}
function getUserAgent() {
  return typeof navigator === 'undefined' ? '' : navigator.userAgent
}
function delay(milliseconds: number, signal?: AbortSignal) {
  return new Promise<void>((resolve) => {
    const timer = globalThis.setTimeout(resolve, milliseconds)
    signal?.addEventListener(
      'abort',
      () => {
        globalThis.clearTimeout(timer)
        resolve()
      },
      { once: true }
    )
  })
}
function formatUptime(seconds: number) {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const rest = seconds % 60
  return [hours, minutes, rest].map((value) => String(value).padStart(2, '0')).join(':')
}
