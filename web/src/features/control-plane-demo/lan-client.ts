import type { ControlPlaneStatus, Identity, NetworkSummary } from './contracts'
import { ControlPlaneClientError } from './transport-client'
import type { DeviceTarget } from './types'

const LAN_STORAGE_PREFIX = 'flux-purr:lan-device:'
const LAN_LEASE_TTL_MS = 30_000

export interface LanPairingMetadata {
  active: boolean
  attemptsRemaining: number
}

export interface LanDeviceSession {
  baseUrl: string
  token: string
  hostname?: string
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

export function isDirectLanDevice(device: Pick<DeviceTarget, 'baseUrl' | 'transport'>) {
  return device.transport === 'wifi' && device.baseUrl.startsWith('http://')
}

export function lanProbeToDeviceTarget(session: LanDeviceSession, probe: LanProbe): DeviceTarget {
  const { identity, network, status } = probe
  return {
    id: `lan-${identity.deviceId}`,
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
    wifiRssi: network.wifiRssi ?? null,
    capabilities: Array.from(new Set([...identity.capabilities, 'lan_http', 'lan_lease'])),
    networkState: network.state,
    leaseState: 'none',
  }
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
  return lanRequest(fetcher, normalizeLanBaseUrl(baseUrl), '/api/v1/pairing')
}

export async function claimLanPairing(
  baseUrl: string,
  code: string,
  fetcher: typeof fetch = fetch
): Promise<LanDeviceSession> {
  if (!/^\d{4}$/.test(code)) {
    throw new ControlPlaneClientError('配对码必须是四位数字。', 'pairing_code_invalid', false)
  }
  const normalizedBaseUrl = normalizeLanBaseUrl(baseUrl)
  const response = await lanRequest<{ token: string; hostname?: string }>(
    fetcher,
    normalizedBaseUrl,
    '/api/v1/pairing/claim',
    { method: 'POST', body: JSON.stringify({ code }) }
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
  fetcher: typeof fetch = fetch
): Promise<LanProbe> {
  const headers = bearerHeaders(session)
  const [identity, network, status] = await Promise.all([
    lanRequest<Identity>(fetcher, session.baseUrl, '/api/v1/identity', { headers }),
    lanRequest<NetworkSummary>(fetcher, session.baseUrl, '/api/v1/network', { headers }),
    lanRequest<ControlPlaneStatus>(fetcher, session.baseUrl, '/api/v1/status', { headers }),
  ])
  return { identity, network, status }
}

export async function writeLanRuntime(
  session: LanDeviceSession,
  leaseId: string,
  body: Record<string, unknown>,
  fetcher: typeof fetch = fetch
) {
  return lanRequest<ControlPlaneStatus>(fetcher, session.baseUrl, '/api/v1/runtime', {
    method: 'PUT',
    headers: { ...bearerHeaders(session), 'X-Flux-Purr-Lease': leaseId },
    body: JSON.stringify(body),
  })
}

export async function authorizedLanRequest<T = Record<string, unknown>>(
  session: LanDeviceSession,
  path: string,
  method = 'GET',
  body?: Record<string, unknown>,
  leaseId?: string,
  fetcher: typeof fetch = fetch
) {
  return lanRequest<T>(fetcher, session.baseUrl, `/api/v1/${path.replace(/^\//, '')}`, {
    method,
    headers: { ...bearerHeaders(session), ...(leaseId ? { 'X-Flux-Purr-Lease': leaseId } : {}) },
    ...(body ? { body: JSON.stringify(body) } : {}),
  })
}

export async function* streamLanEvents(
  session: LanDeviceSession,
  signal?: AbortSignal,
  fetcher: typeof fetch = fetch
): AsyncGenerator<Record<string, unknown>> {
  while (!signal?.aborted) {
    const request = {
      headers: { ...bearerHeaders(session), Accept: 'text/event-stream' },
      signal,
      targetAddressSpace: 'private',
    } satisfies PrivateNetworkRequestInit
    const response = await fetcher(`${session.baseUrl}/api/v1/events`, request as RequestInit)
    if (!response.ok || !response.body) throw await responseError(response)
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
    if (!signal?.aborted) await delay(1_000, signal)
  }
}

async function lanRequest<T = Record<string, unknown>>(
  fetcher: typeof fetch,
  baseUrl: string,
  path: string,
  init: RequestInit = {}
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
  const response = await fetcher(`${baseUrl}${path}`, request as RequestInit)
  if (!response.ok) throw await responseError(response)
  return (await response.json()) as T
}

function bearerHeaders(session: LanDeviceSession) {
  return { Authorization: `Bearer ${session.token}` }
}

async function responseError(response: Response) {
  const payload = (await response.json().catch(() => null)) as {
    error?: { code?: string; message?: string; retryable?: boolean }
  } | null
  if (response.status === 401) forgetLanDeviceSession(response.url.replace(/\/api\/v1\/.*$/, ''))
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
function getStorage() {
  return typeof window === 'undefined' ? null : window.localStorage
}
function getUserAgent() {
  return typeof navigator === 'undefined' ? '' : navigator.userAgent
}
function delay(milliseconds: number, signal?: AbortSignal) {
  return new Promise<void>((resolve) => {
    const timer = window.setTimeout(resolve, milliseconds)
    signal?.addEventListener(
      'abort',
      () => {
        window.clearTimeout(timer)
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
